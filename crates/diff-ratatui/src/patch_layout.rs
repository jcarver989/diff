//! Diff-specific adaptation from review annotations to visual patch rows.

use crate::annotation::{AnnotationBox, AnnotationKind, AnnotationLayout, AnnotationRow};
use diff_core::{CommentDraft, ReviewComment, ReviewSession};
use std::{collections::BTreeMap, ops::Range};

pub(crate) type PatchVisualRow<'a> = AnnotationRow<'a>;

/// Adapts diff review comments and drafts to the generic annotation layout.
#[derive(Debug, Default)]
pub(crate) struct PatchVisualLayout {
    layout: AnnotationLayout,
}

impl PatchVisualLayout {
    pub(crate) fn new(session: &ReviewSession, range: Range<usize>, width: u16) -> Self {
        let mut anchored: BTreeMap<usize, Vec<AnnotationBox>> = BTreeMap::new();
        for comment in session.review().comments() {
            if let Some(row) = session
                .presentation()
                .row_showing_anchor(&comment.anchor)
                .filter(|row| range.contains(row))
            {
                anchored
                    .entry(row)
                    .or_default()
                    .push(comment_box(comment, width));
            }
        }
        if let Some(draft) = session.draft()
            && let Some(row) = session
                .presentation()
                .row_showing_anchor(draft.anchor())
                .filter(|row| range.contains(row))
        {
            anchored
                .entry(row)
                .or_default()
                .push(draft_box(draft, width));
        }
        Self {
            layout: AnnotationLayout::new(range, anchored),
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.layout.len()
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.layout.is_empty()
    }

    pub(crate) fn focused_visual_row(&self, source: usize, draft: bool) -> Option<usize> {
        self.layout.focused_visual_row(source, draft)
    }

    pub(crate) fn row(&self, visual: usize) -> Option<PatchVisualRow<'_>> {
        self.layout.row(visual)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use diff_core::testing::DocumentBuilder;

    #[test]
    fn adapts_diff_comments_without_changing_their_order() {
        let document = DocumentBuilder::new()
            .changed("src/main.rs", "old\n", "new\n")
            .build();
        let mut session = ReviewSession::new(document);
        let source = (0..session.presentation().row_count())
            .find(|index| session.presentation().is_commentable(*index))
            .unwrap();
        let row = session.presentation().row(source).unwrap();
        let cell = row.primary_cell().unwrap();
        let anchor = session.presentation().cell_anchor(row, cell).unwrap();
        session.review_mut().add_comment(anchor, "comment");

        let source_rows = session.presentation().row_count();
        let layout = PatchVisualLayout::new(&session, 0..source_rows, 40);
        assert_eq!(layout.len(), source_rows + 3);
        assert!(matches!(
            layout.row(source),
            Some(PatchVisualRow::Source(_))
        ));
        assert!(matches!(
            layout.row(source + 1),
            Some(PatchVisualRow::Annotation { .. })
        ));
    }
}
