use crate::{
    MarkdownLayoutOptions, MarkdownRenderer, MarkdownRow, MarkdownRows,
    annotation::{AnnotationBox, AnnotationKind},
    annotation_layout::{AnnotationLayout, AnnotationRow},
};
use clankerdiff_markdown::{MarkdownAnchors, MarkdownReviewSession, MarkdownTargetId};
use clankerdiff_ui_core::ReviewTheme;
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::Arc,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum MarkdownVisualRow<'a> {
    Content(&'a Arc<MarkdownRow>),
    Annotation { annotation: &'a AnnotationBox, line: usize, target: Option<MarkdownTargetId> },
}

impl MarkdownVisualRow<'_> {
    pub(crate) fn target(&self) -> Option<MarkdownTargetId> {
        match self {
            Self::Content(row) => row.target,
            Self::Annotation { target, .. } => *target,
        }
    }

    pub(crate) fn source_line(&self) -> Option<usize> {
        match self {
            Self::Content(row) => row.source.as_ref().map(|source| source.lines.start),
            Self::Annotation { .. } => None,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct MarkdownContentLayout {
    rows: MarkdownRows,
    ranges: HashMap<MarkdownTargetId, Range<usize>>,
}

impl MarkdownContentLayout {
    pub(crate) fn build(session: &MarkdownReviewSession, width: u16, theme: &ReviewTheme) -> Self {
        let document = session.document();
        let layout = MarkdownRenderer::new().render_layout(
            document,
            MarkdownLayoutOptions {
                width,
                block_spacing: false,
                preserve_source_gaps: true,
                ..MarkdownLayoutOptions::default()
            },
            theme,
        );
        let ranges = document
            .targets()
            .iter()
            .filter_map(|target| Some((target.id, layout.rows_for_target(target.id)?)))
            .collect();
        Self { rows: layout.rows().clone(), ranges }
    }
}

#[derive(Debug, Default)]
pub(crate) struct MarkdownVisualLayout {
    content: Arc<MarkdownContentLayout>,
    annotations: AnnotationLayout,
}

impl MarkdownVisualLayout {
    pub(crate) fn new(session: &MarkdownReviewSession, content: Arc<MarkdownContentLayout>, width: u16) -> Self {
        let document = session.document();
        let mut anchored: BTreeMap<usize, Vec<AnnotationBox>> = BTreeMap::new();
        let mut anchor = |target, annotation| {
            if let Some(range) = content.ranges.get(&target) {
                anchored.entry(range.end - 1).or_default().push(annotation);
            }
        };
        for comment in session.review().comments() {
            if let Some(target) = document.exact_target(&comment.anchor) {
                let (kind, title) = if comment.outdated {
                    (AnnotationKind::Outdated, "Outdated comment")
                } else {
                    (AnnotationKind::Comment, "Comment")
                };
                anchor(target, AnnotationBox::new(kind, title, &comment.body, None, width));
            }
        }
        if let Some(draft) = session.draft() {
            let annotation =
                AnnotationBox::new(AnnotationKind::Draft, "Draft", draft.body(), Some(draft.cursor()), width);
            anchor(draft.target(), annotation);
        }
        let annotations = AnnotationLayout::new(0..content.rows.len(), anchored);
        Self { content, annotations }
    }

    pub(crate) const fn len(&self) -> usize {
        self.annotations.len()
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }

    pub(crate) fn focused_row(&self, target: MarkdownTargetId, drafting: bool) -> Option<usize> {
        let range = self.content.ranges.get(&target)?;
        if drafting {
            self.annotations.focused_visual_row(range.end - 1, true)
        } else {
            self.annotations.visual_offset_for_source(range.start)
        }
    }

    pub(crate) fn row(&self, visual: usize) -> Option<MarkdownVisualRow<'_>> {
        Some(match self.annotations.row(visual)? {
            AnnotationRow::Source(index) => MarkdownVisualRow::Content(self.content.rows.get(index)?),
            AnnotationRow::Annotation { source, annotation, line } => MarkdownVisualRow::Annotation {
                annotation,
                line,
                target: self.content.rows.get(source).and_then(|row| row.target),
            },
        })
    }
}
