use crate::{
    annotation::{AnnotationBox, AnnotationKind},
    annotation_layout::{AnnotationLayout, AnnotationRow},
    text::{FitCursor, FitOptions, FitPosition},
};
use clankerdiff_core::{
    CommentDraft, DiffPresentation, DiffSide, Layout, PresentedCell, ReviewComment, ReviewSession,
    RowKind,
};
use std::{collections::BTreeMap, ops::Range, sync::Arc};

const MIN_GUTTER_WIDTH: u16 = 6;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PatchPaneGeometry {
    pub(crate) width: u16,
    pub(crate) gutter_width: u16,
    pub(crate) content_width: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PatchRowGeometry {
    pub(crate) split: bool,
    pub(crate) left: PatchPaneGeometry,
    pub(crate) right: PatchPaneGeometry,
}

impl PatchRowGeometry {
    pub(crate) fn new(width: u16, layout: Layout, number_digits: usize) -> Self {
        let gutter_width = MIN_GUTTER_WIDTH
            .max(u16::try_from(number_digits.saturating_add(2)).unwrap_or(u16::MAX));
        let split = layout.is_split();
        let (left_width, right_width) = if split {
            let left = width.saturating_sub(1) / 2;
            (left, width.saturating_sub(left.saturating_add(1)))
        } else {
            (0, width)
        };
        let pane = |pane_width| PatchPaneGeometry {
            width: pane_width,
            gutter_width: gutter_width.min(pane_width),
            content_width: usize::from(pane_width.saturating_sub(gutter_width)),
        };
        Self {
            split,
            left: pane(left_width),
            right: pane(right_width),
        }
    }
}

#[derive(Debug)]
struct PatchContentRow {
    source: usize,
    range: Range<usize>,
    left: Vec<FitPosition>,
    right: Vec<FitPosition>,
}

#[derive(Debug)]
pub(crate) struct PatchContentLayout {
    rows: Vec<PatchContentRow>,
    source_range: Range<usize>,
    len: usize,
    geometry: PatchRowGeometry,
}

impl PatchContentLayout {
    pub(crate) fn new(
        presentation: &DiffPresentation,
        range: Range<usize>,
        width: u16,
        tab_width: u16,
    ) -> Self {
        let number_digits = presentation
            .rows(range.clone())
            .iter()
            .flat_map(clankerdiff_core::PresentedRow::cells)
            .filter_map(PresentedCell::line_number)
            .max()
            .map_or(1, |number| number.to_string().len());
        let geometry = PatchRowGeometry::new(width, presentation.layout(), number_digits);
        let mut rows = Vec::with_capacity(range.len());
        let mut offset: usize = 0;
        for (index, row) in presentation.rows(range.clone()).iter().enumerate() {
            let source = range.start + index;
            let wrapping = matches!(row.kind, RowKind::Code | RowKind::ExpandedContext);
            let (left, right) = if wrapping && geometry.split {
                (
                    measure_cell(row.left.as_ref(), geometry.left.content_width, tab_width),
                    measure_cell(row.right.as_ref(), geometry.right.content_width, tab_width),
                )
            } else if wrapping {
                (
                    Vec::new(),
                    measure_cell(row.primary_cell(), geometry.right.content_width, tab_width),
                )
            } else {
                (Vec::new(), Vec::new())
            };
            let height = left.len().max(right.len()).max(1);
            let visual_range = offset..offset.saturating_add(height);
            offset = visual_range.end;
            rows.push(PatchContentRow {
                source,
                range: visual_range,
                left,
                right,
            });
        }
        Self {
            rows,
            source_range: range,
            len: offset,
            geometry,
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    pub(crate) const fn geometry(&self) -> PatchRowGeometry {
        self.geometry
    }

    pub(crate) fn content_range_for_source(&self, source: usize) -> Option<Range<usize>> {
        let index = source.checked_sub(self.source_range.start)?;
        self.rows.get(index).map(|row| row.range.clone())
    }

    pub(crate) fn source_at_content_row(&self, content: usize) -> Option<(usize, usize)> {
        let index = self.rows.partition_point(|row| row.range.end <= content);
        let row = self.rows.get(index)?;
        Some((row.source, content - row.range.start))
    }

    pub(crate) fn checkpoint(
        &self,
        source: usize,
        side: DiffSide,
        segment: usize,
    ) -> Option<FitPosition> {
        let index = source.checked_sub(self.source_range.start)?;
        let row = self.rows.get(index)?;
        let checkpoints = match side {
            DiffSide::Old => &row.left,
            DiffSide::New => &row.right,
        };
        checkpoints.get(segment).copied()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PatchVisualRow<'a> {
    Source {
        index: usize,
        segment: usize,
    },
    Annotation {
        source: usize,
        annotation: &'a AnnotationBox,
        line: usize,
    },
}

#[derive(Debug)]
pub(crate) struct PatchVisualLayout {
    content: Arc<PatchContentLayout>,
    layout: AnnotationLayout,
}

impl PatchVisualLayout {
    pub(crate) fn new(
        session: &ReviewSession,
        content: Arc<PatchContentLayout>,
        width: u16,
    ) -> Self {
        let mut anchored: BTreeMap<usize, Vec<AnnotationBox>> = BTreeMap::new();
        for comment in session.review().comments() {
            if let Some(source) = session.presentation().row_showing_anchor(&comment.anchor)
                && let Some(range) = content.content_range_for_source(source)
            {
                anchored
                    .entry(range.end - 1)
                    .or_default()
                    .push(comment_box(comment, width));
            }
        }
        if let Some(draft) = session.draft()
            && let Some(source) = session.presentation().row_showing_anchor(draft.anchor())
            && let Some(range) = content.content_range_for_source(source)
        {
            anchored
                .entry(range.end - 1)
                .or_default()
                .push(draft_box(draft, width));
        }
        let layout = AnnotationLayout::new(0..content.len(), anchored);
        Self { content, layout }
    }

    pub(crate) const fn len(&self) -> usize {
        self.layout.len()
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.layout.is_empty()
    }

    pub(crate) fn content(&self) -> &PatchContentLayout {
        &self.content
    }

    pub(crate) fn focused_visual_row(&self, source: usize, draft: bool) -> Option<usize> {
        let range = self.content.content_range_for_source(source)?;
        let content = if draft { range.end - 1 } else { range.start };
        self.layout.focused_visual_row(content, draft)
    }

    pub(crate) fn row(&self, visual: usize) -> Option<PatchVisualRow<'_>> {
        match self.layout.row(visual)? {
            AnnotationRow::Source(content) => {
                let (index, segment) = self.content.source_at_content_row(content)?;
                Some(PatchVisualRow::Source { index, segment })
            }
            AnnotationRow::Annotation {
                source: content,
                annotation,
                line,
            } => {
                let (source, _) = self.content.source_at_content_row(content)?;
                Some(PatchVisualRow::Annotation {
                    source,
                    annotation,
                    line,
                })
            }
        }
    }
}

fn measure_cell(cell: Option<&PresentedCell>, width: usize, tab_width: u16) -> Vec<FitPosition> {
    let Some(cell) = cell else {
        return Vec::new();
    };
    let mut cursor = FitCursor::new(
        FitOptions {
            width,
            wrap: true,
            tab_width: usize::from(tab_width),
            continuation: "",
        },
        FitPosition::default(),
    );
    let mut checkpoints = Vec::new();
    loop {
        let start = cursor.position();
        if cursor.next_row(&cell.text, |_, _| {}).is_none() {
            break;
        }
        checkpoints.push(start);
    }
    checkpoints
}

fn comment_box(comment: &ReviewComment, width: u16) -> AnnotationBox {
    let kind = if comment.outdated {
        AnnotationKind::Outdated
    } else {
        AnnotationKind::Comment
    };
    let title = if comment.outdated {
        "Outdated comment"
    } else {
        "Comment"
    };
    AnnotationBox::new(kind, title, &comment.body, None, width)
}

fn draft_box(draft: &CommentDraft, width: u16) -> AnnotationBox {
    AnnotationBox::new(
        AnnotationKind::Draft,
        "Draft",
        draft.body(),
        Some(draft.cursor()),
        width,
    )
}
