//! Host-neutral comment draft state used by the GPUI view.

use diff_core::LineAnchor;

#[derive(Clone)]
pub(crate) struct CommentDraft {
    pub anchor: LineAnchor,
    pub line_text: String,
    pub body: String,
    pub editing: Option<u64>,
}

impl CommentDraft {
    pub fn new(anchor: LineAnchor, line_text: String) -> Self {
        Self {
            anchor,
            line_text,
            body: String::new(),
            editing: None,
        }
    }

    pub fn editing(anchor: LineAnchor, line_text: String, body: String, comment_id: u64) -> Self {
        Self {
            anchor,
            line_text,
            body,
            editing: Some(comment_id),
        }
    }

    pub fn backspace(&mut self) {
        self.body.pop();
    }

    pub fn push(&mut self, text: &str) {
        self.body.push_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_core::{DiffSide, FileDiff, LineAnchor};

    #[test]
    fn backspace_removes_a_whole_unicode_scalar() {
        let file = FileDiff::from_texts("a.rs", "old", "new").unwrap();
        let anchor = LineAnchor::for_line(&file, DiffSide::New, 0, 1).unwrap();
        let mut draft = CommentDraft::new(anchor, "new".into());
        draft.push("café");
        draft.backspace();
        assert_eq!(draft.body, "caf");
    }
}
