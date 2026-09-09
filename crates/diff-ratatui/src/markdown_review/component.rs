use super::{MarkdownReviewEvent, MarkdownReviewState, MarkdownReviewWidget};
use crate::{
    InputOutcome, KeyBinding, KeyEvent, MarkdownReviewCommand, NavigationPane, ReviewInput,
    ReviewOptions, ThemeChoice, default_markdown_keybindings, keybindings::markdown_command_label,
};
use clankerdiff_core::ReviewCapabilities;
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewError};
use clankerdiff_theme::ReviewTheme;
#[cfg(feature = "crossterm-backend")]
use crossterm::event::Event;
use ratatui::{Frame, layout::Rect};
use std::sync::Arc;

#[derive(Debug)]
pub struct MarkdownReview {
    state: MarkdownReviewState,
    widget: MarkdownReviewWidget,
}

impl MarkdownReview {
    #[must_use]
    pub fn builder() -> MarkdownReviewBuilder {
        MarkdownReviewBuilder::default()
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_stateful_widget(self.widget.clone(), area, &mut self.state);
        if let Some(position) = self.state.cursor_position() {
            frame.set_cursor_position(position);
        }
    }

    pub fn handle_input(
        &mut self,
        input: impl Into<ReviewInput>,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        self.state.handle_input(input.into())
    }

    pub fn handle_command(
        &mut self,
        command: impl Into<MarkdownReviewCommand>,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        self.state.handle_command(command)
    }

    #[cfg(feature = "crossterm-backend")]
    pub fn handle_crossterm_event(
        &mut self,
        event: &Event,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        super::handle_crossterm_event(&mut self.state, event.clone())
    }

    #[must_use]
    pub const fn state(&self) -> &MarkdownReviewState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut MarkdownReviewState {
        self.state.mark_dirty();
        &mut self.state
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.state.is_dirty()
    }

    pub fn mark_dirty(&mut self) {
        self.state.mark_dirty();
    }

    pub fn set_document(&mut self, document: impl Into<Arc<MarkdownDocument>>) {
        self.state.set_document(document.into());
    }

    pub fn set_theme(&mut self, theme: ReviewTheme) {
        self.state.set_theme(theme);
    }
}

#[derive(Debug)]
pub struct MarkdownReviewBuilder<T = ()> {
    document: T,
    theme: ReviewTheme,
    theme_choices: Arc<[ThemeChoice]>,
    options: ReviewOptions,
    capabilities: ReviewCapabilities,
    keybindings: Vec<KeyBinding<MarkdownReviewCommand>>,
    widget: MarkdownReviewWidget,
}

impl Default for MarkdownReviewBuilder {
    fn default() -> Self {
        Self {
            document: (),
            theme: ReviewTheme::default(),
            theme_choices: Arc::from([]),
            options: ReviewOptions::default(),
            capabilities: ReviewCapabilities::default(),
            keybindings: default_markdown_keybindings(),
            widget: MarkdownReviewWidget::new(),
        }
    }
}

impl<T> MarkdownReviewBuilder<T> {
    #[must_use]
    pub fn markdown(self, source: impl AsRef<str>) -> MarkdownReviewBuilder<Arc<MarkdownDocument>> {
        self.document(MarkdownDocument::parse(source.as_ref()))
    }

    #[must_use]
    pub fn document(
        self,
        document: impl Into<Arc<MarkdownDocument>>,
    ) -> MarkdownReviewBuilder<Arc<MarkdownDocument>> {
        MarkdownReviewBuilder {
            document: document.into(),
            theme: self.theme,
            theme_choices: self.theme_choices,
            options: self.options,
            capabilities: self.capabilities,
            keybindings: self.keybindings,
            widget: self.widget,
        }
    }

    #[must_use]
    pub fn embedded(self) -> Self {
        self.borders(false)
            .footer(false)
            .navigation(NavigationPane::Hidden)
    }

    #[must_use]
    pub fn theme(mut self, theme: ReviewTheme) -> Self {
        self.theme = theme;
        self
    }

    #[must_use]
    pub fn theme_choices(mut self, themes: impl Into<Arc<[ThemeChoice]>>) -> Self {
        self.theme_choices = themes.into();
        self
    }

    #[must_use]
    pub fn options(mut self, options: ReviewOptions) -> Self {
        self.options = options;
        self
    }

    #[must_use]
    pub fn footer(mut self, visible: bool) -> Self {
        self.options.footer = visible;
        self
    }

    #[must_use]
    pub fn navigation(mut self, navigation: NavigationPane) -> Self {
        self.options.navigation = navigation;
        self
    }

    #[must_use]
    pub fn borders(mut self, visible: bool) -> Self {
        self.widget = self.widget.borders(visible);
        self
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.widget = self.widget.title(title);
        self
    }

    #[must_use]
    pub fn capabilities(mut self, capabilities: ReviewCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    #[must_use]
    pub fn bind(self, key: impl Into<KeyEvent>, command: impl Into<MarkdownReviewCommand>) -> Self {
        let command = command.into();
        self.binding(KeyBinding::new(
            key.into(),
            command,
            markdown_command_label(command),
        ))
    }

    #[must_use]
    pub fn binding(mut self, binding: KeyBinding<MarkdownReviewCommand>) -> Self {
        self.keybindings.push(binding);
        self
    }

    #[must_use]
    pub fn keybindings(
        mut self,
        bindings: impl Into<Vec<KeyBinding<MarkdownReviewCommand>>>,
    ) -> Self {
        self.keybindings = bindings.into();
        self
    }
}

impl MarkdownReviewBuilder<Arc<MarkdownDocument>> {
    #[must_use]
    pub fn build(self) -> MarkdownReview {
        let mut state = MarkdownReviewState::with_theme(self.document, self.theme);
        state.set_options(self.options);
        state.set_capabilities(self.capabilities);
        state.set_keybindings(self.keybindings);
        state.set_theme_choices(self.theme_choices);
        MarkdownReview {
            state,
            widget: self.widget,
        }
    }
}
