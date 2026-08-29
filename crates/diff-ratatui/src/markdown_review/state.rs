use super::layout::MarkdownVisualLayout;
use crate::theme_picker::ThemePicker;
use diff_core::{
    DiffTheme, HighlightStats, MarkdownDocument, MarkdownReview, MarkdownReviewSession,
    MarkdownTargetId,
};
use ratatui::layout::{Position, Rect};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

/// The pane receiving Markdown review navigation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarkdownFocusPane {
    /// The rendered document.
    #[default]
    Document,
    /// The heading outline.
    Outline,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MarkdownHitRegion {
    pub area: Rect,
    pub target: Option<MarkdownTargetId>,
    pub outline: bool,
}

#[derive(Debug)]
struct CachedLayout {
    key: u64,
    width: u16,
    layout: Arc<MarkdownVisualLayout>,
}

/// Persistent state for [`crate::MarkdownReviewWidget`].
#[derive(Debug)]
pub struct MarkdownReviewState {
    pub(crate) session: MarkdownReviewSession,
    pub(crate) theme: DiffTheme,
    pub(crate) highlighter: diff_core::SyntaxHighlighter,
    pub(crate) focus: MarkdownFocusPane,
    pub(crate) scroll: usize,
    pub(crate) outline_scroll: usize,
    pub(crate) outline_selected: usize,
    pub(crate) last_height: usize,
    layout: Option<CachedLayout>,
    pub(crate) hit_regions: Vec<MarkdownHitRegion>,
    pub(crate) cursor_position: Option<Position>,
    pub(crate) help: bool,
    pub(crate) theme_picker: Option<ThemePicker>,
    pub(crate) dirty: bool,
    pub(crate) follow_pending: bool,
}

impl MarkdownReviewState {
    /// Creates ready state from an immutable parsed document.
    #[must_use]
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        Self::with_theme(document, DiffTheme::default())
    }

    /// Creates state with an explicit shared neutral theme.
    #[must_use]
    pub fn with_theme(document: Arc<MarkdownDocument>, theme: DiffTheme) -> Self {
        Self {
            session: MarkdownReviewSession::new(document),
            theme,
            highlighter: diff_core::SyntaxHighlighter::default(),
            focus: MarkdownFocusPane::Document,
            scroll: 0,
            outline_scroll: 0,
            outline_selected: 0,
            last_height: 1,
            layout: None,
            hit_regions: Vec::new(),
            cursor_position: None,
            help: false,
            theme_picker: None,
            dirty: true,
            follow_pending: true,
        }
    }

    #[must_use]
    pub const fn session(&self) -> &MarkdownReviewSession {
        &self.session
    }

    pub const fn session_mut(&mut self) -> &mut MarkdownReviewSession {
        &mut self.session
    }

    #[must_use]
    pub const fn document(&self) -> &Arc<MarkdownDocument> {
        self.session.document()
    }

    #[must_use]
    pub const fn review(&self) -> &MarkdownReview {
        self.session.review()
    }

    pub const fn review_mut(&mut self) -> &mut MarkdownReview {
        self.session.review_mut()
    }

    #[must_use]
    pub const fn selected_target(&self) -> Option<MarkdownTargetId> {
        self.session.selected_target()
    }

    #[must_use]
    pub const fn focus(&self) -> MarkdownFocusPane {
        self.focus
    }

    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.scroll
    }

    #[must_use]
    pub const fn outline_scroll_offset(&self) -> usize {
        self.outline_scroll
    }

    #[must_use]
    pub const fn cursor_position(&self) -> Option<Position> {
        self.cursor_position
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub const fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Returns the active renderer-neutral theme.
    #[must_use]
    pub const fn theme(&self) -> &DiffTheme {
        &self.theme
    }

    #[must_use]
    pub const fn highlight_stats(&self) -> HighlightStats {
        self.highlighter.stats()
    }

    /// Replaces the parsed snapshot and reconciles all existing comments.
    pub fn set_document(&mut self, document: Arc<MarkdownDocument>) {
        self.session.replace_document(document);
        self.layout = None;
        self.cursor_position = None;
        self.scroll = 0;
        self.follow_pending = true;
        self.mark_dirty();
    }

    /// Changes the theme and invalidates syntax highlighting.
    pub fn set_theme(&mut self, theme: DiffTheme) {
        self.theme = theme;
        self.highlighter.clear_cache();
        self.mark_dirty();
    }

    pub(crate) fn ensure_layout(&mut self, width: u16) -> Arc<MarkdownVisualLayout> {
        let key = self.layout_key(width);
        if self
            .layout
            .as_ref()
            .is_none_or(|cached| cached.key != key || cached.width != width)
        {
            self.layout = Some(CachedLayout {
                key,
                width,
                layout: Arc::new(MarkdownVisualLayout::build(&self.session, width)),
            });
        }
        Arc::clone(&self.layout.as_ref().expect("layout inserted above").layout)
    }

    fn layout_key(&self, width: u16) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.document().source().hash(&mut hasher);
        width.hash(&mut hasher);
        self.review().comments().iter().for_each(|comment| {
            comment.id.hash(&mut hasher);
            comment.body.hash(&mut hasher);
            comment.outdated.hash(&mut hasher);
            format!("{:?}", comment.anchor).hash(&mut hasher);
        });
        if let Some(draft) = self.session.draft() {
            draft.target().hash(&mut hasher);
            draft.body().hash(&mut hasher);
            draft.cursor().hash(&mut hasher);
        }
        hasher.finish()
    }

    pub(crate) fn request_follow(&mut self) {
        self.follow_pending = true;
        self.mark_dirty();
    }

    pub(crate) fn follow_selection(&mut self, layout: &MarkdownVisualLayout) {
        if !self.follow_pending {
            return;
        }
        self.follow_pending = false;
        let Some(target) = self.selected_target() else {
            return;
        };
        let Some(row) = layout.row_for_target(target) else {
            return;
        };
        let height = self.last_height.max(1);
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll.saturating_add(height) {
            self.scroll = row.saturating_sub(height - 1);
        }
    }

    pub(crate) fn selected_outline_target(&self) -> Option<MarkdownTargetId> {
        self.document()
            .outline()
            .get(self.outline_selected)
            .map(|heading| heading.target_id)
    }

    pub(crate) fn clear_hit_regions(&mut self) {
        self.hit_regions.clear();
    }

    pub(crate) fn set_cursor(&mut self, position: Option<Position>) {
        self.cursor_position = position;
    }
}
