use crate::{
    MarkdownLayoutOptions, MarkdownRenderer, MarkdownRow, MarkdownRows,
    annotation::{AnnotationBox, AnnotationKind},
    annotation_layout::{AnnotationLayout, AnnotationRow},
};
use clankerdiff_markdown::{MarkdownReviewSession, MarkdownTargetId};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::ReviewTheme;
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::Arc,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum MarkdownVisualRow<'a> {
    Content(&'a Arc<MarkdownRow>),
    Annotation {
        annotation: &'a AnnotationBox,
        line: usize,
        target: Option<MarkdownTargetId>,
    },
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
pub(crate) struct MarkdownVisualLayout {
    content: MarkdownRows,
    annotations: AnnotationLayout,
    ranges: HashMap<MarkdownTargetId, Range<usize>>,
}

impl MarkdownVisualLayout {
    pub(crate) fn build(
        session: &MarkdownReviewSession,
        width: u16,
        highlighter: &mut SyntaxHighlighter,
        theme: &ReviewTheme,
    ) -> Self {
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
            highlighter,
        );
        let ranges = document
            .targets()
            .iter()
            .filter_map(|target| Some((target.id, layout.rows_for_target(target.id)?)))
            .collect::<Vec<(MarkdownTargetId, Range<usize>)>>();
        let mut anchored: BTreeMap<usize, Vec<AnnotationBox>> = BTreeMap::new();
        for (target, range) in &ranges {
            let boxes = annotation_boxes(session, *target, width);
            if !boxes.is_empty() {
                anchored.entry(range.end - 1).or_default().extend(boxes);
            }
        }
        Self {
            content: layout.rows().clone(),
            annotations: AnnotationLayout::new(0..layout.row_count(), anchored),
            ranges: ranges.into_iter().collect(),
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.annotations.len()
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }

    pub(crate) fn focused_row(&self, target: MarkdownTargetId, drafting: bool) -> Option<usize> {
        let range = self.ranges.get(&target)?;
        if drafting {
            self.annotations.focused_visual_row(range.end - 1, true)
        } else {
            self.annotations.visual_offset_for_source(range.start)
        }
    }

    pub(crate) fn row(&self, visual: usize) -> Option<MarkdownVisualRow<'_>> {
        Some(match self.annotations.row(visual)? {
            AnnotationRow::Source(index) => MarkdownVisualRow::Content(self.content.get(index)?),
            AnnotationRow::Annotation {
                source,
                annotation,
                line,
            } => MarkdownVisualRow::Annotation {
                annotation,
                line,
                target: self.content.get(source).and_then(|row| row.target),
            },
        })
    }
}

fn annotation_boxes(
    session: &MarkdownReviewSession,
    target: MarkdownTargetId,
    width: u16,
) -> Vec<AnnotationBox> {
    let mut boxes = session
        .review()
        .comments_for_target(session.document(), target)
        .map(|comment| {
            let (kind, title) = if comment.outdated {
                (AnnotationKind::Outdated, "Outdated comment")
            } else {
                (AnnotationKind::Comment, "Comment")
            };
            AnnotationBox::new(kind, title, &comment.body, None, width)
        })
        .collect::<Vec<_>>();
    if let Some(draft) = session.draft().filter(|draft| draft.target() == target) {
        boxes.push(AnnotationBox::new(
            AnnotationKind::Draft,
            "Draft",
            draft.body(),
            Some(draft.cursor()),
            width,
        ));
    }
    boxes
}
