use super::layout::MarkdownVisualLayout;
use crate::{
    KeyBinding, MarkdownReviewCommand, NavigationPane, ReviewOptions, ThemeChoice,
    default_markdown_keybindings, theme_picker::ThemePicker,
};
use clankerdiff_core::ReviewCapabilities;
use clankerdiff_markdown::{
    MarkdownDocument, MarkdownReview, MarkdownReviewSession, MarkdownTargetId,
};
use clankerdiff_syntax::{HighlightStats, SyntaxHighlighter};
use clankerdiff_theme::ReviewTheme;
use ratatui::layout::{Position, Rect};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

pub use clankerdiff_markdown::MarkdownFocusPane;

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
    pub(crate) options: ReviewOptions,
    pub(crate) capabilities: ReviewCapabilities,
    pub(crate) keybindings: Vec<KeyBinding<MarkdownReviewCommand>>,
    pub(crate) theme_choices: Arc<[ThemeChoice]>,
    pub(crate) theme: ReviewTheme,
    pub(crate) highlighter: SyntaxHighlighter,
    pub(crate) focus: MarkdownFocusPane,
    pub(crate) scroll: usize,
    pub(crate) outline_scroll: usize,
    pub(crate) outline_selected: usize,
    pub(crate) last_height: usize,
    layout: Option<CachedLayout>,
    pub(crate) hit_regions: Vec<MarkdownHitRegion>,
    pub(crate) cursor_position: Option<Position>,
    pub(crate) help: bool,
    pub(crate) help_scroll: usize,
    pub(crate) theme_picker: Option<ThemePicker>,
    pub(crate) dirty: bool,
    pub(crate) follow_pending: bool,
}

impl MarkdownReviewState {
    /// Creates ready state from an immutable parsed document.
    #[must_use]
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        Self::with_theme(document, ReviewTheme::default())
    }

    /// Creates state with an explicit shared neutral theme.
    #[must_use]
    pub fn with_theme(document: Arc<MarkdownDocument>, theme: ReviewTheme) -> Self {
        Self {
            session: MarkdownReviewSession::new(document),
            options: ReviewOptions::default(),
            capabilities: ReviewCapabilities::default(),
            keybindings: default_markdown_keybindings(),
            theme_choices: Arc::from([]),
            theme,
            highlighter: SyntaxHighlighter::default(),
            focus: MarkdownFocusPane::Document,
            scroll: 0,
            outline_scroll: 0,
            outline_selected: 0,
            last_height: 1,
            layout: None,
            hit_regions: Vec::new(),
            cursor_position: None,
            help: false,
            help_scroll: 0,
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
    pub const fn theme(&self) -> &ReviewTheme {
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
    pub fn set_theme(&mut self, theme: ReviewTheme) {
        self.theme_picker = None;
        self.apply_theme(theme);
    }

    pub(crate) fn apply_theme(&mut self, theme: ReviewTheme) {
        self.theme = theme;
        self.highlighter.clear_cache();
        self.mark_dirty();
    }

    #[must_use]
    pub const fn options(&self) -> &ReviewOptions {
        &self.options
    }

    pub fn keybindings(&self) -> &[KeyBinding<MarkdownReviewCommand>] {
        &self.keybindings
    }

    pub fn set_keybindings(&mut self, bindings: impl Into<Vec<KeyBinding<MarkdownReviewCommand>>>) {
        self.keybindings = bindings.into();
        self.help_scroll = 0;
        self.mark_dirty();
    }

    pub fn set_options(&mut self, options: ReviewOptions) {
        if matches!(
            options.navigation,
            NavigationPane::Hidden | NavigationPane::Width(0)
        ) {
            self.focus = MarkdownFocusPane::Document;
        }
        self.options = options;
        self.hit_regions.clear();
        self.request_follow();
    }

    #[must_use]
    pub fn theme_choices(&self) -> &[ThemeChoice] {
        &self.theme_choices
    }

    pub fn set_theme_choices(&mut self, themes: impl Into<Arc<[ThemeChoice]>>) {
        if let Some(picker) = self.theme_picker.take() {
            self.set_theme(picker.cancel());
        }
        self.theme_choices = themes.into();
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
                layout: Arc::new(MarkdownVisualLayout::build(
                    &self.session,
                    width,
                    &mut self.highlighter,
                    &self.theme,
                )),
            });
        }
        Arc::clone(&self.layout.as_ref().expect("layout inserted above").layout)
    }

    fn layout_key(&self, width: u16) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.document().source().hash(&mut hasher);
        width.hash(&mut hasher);
        self.theme.revision().hash(&mut hasher);
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
        let Some(row) = layout.focused_row(target, self.session.draft().is_some()) else {
            return;
        };
        let height = self.last_height.max(1);
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll.saturating_add(height) {
            self.scroll = row.saturating_sub(height - 1);
        }
    }

    pub(crate) fn clear_hit_regions(&mut self) {
        self.hit_regions.clear();
    }

    pub(crate) fn set_cursor(&mut self, position: Option<Position>) {
        self.cursor_position = position;
    }
}
