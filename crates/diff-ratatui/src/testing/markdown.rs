use crate::{
    MarkdownLayout, MarkdownLayoutOptions, MarkdownRenderer, MarkdownRow, MarkdownRowUpdate,
    StreamingMarkdownState,
};
use clankerdiff_markdown::{MarkdownDocument, MarkdownStream};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::ReviewTheme;
use ratatui::text::Line;
use std::sync::Arc;

/// A streaming Markdown item plus the host-side row mirror an embedding keeps.
#[derive(Default)]
pub struct MarkdownStreamFixture {
    pub stream: MarkdownStream,
    pub state: StreamingMarkdownState,
    pub options: MarkdownLayoutOptions,
    pub theme: ReviewTheme,
    pub highlighter: SyntaxHighlighter,
    pub host: Vec<Arc<MarkdownRow>>,
    pub revision: u64,
}

impl MarkdownStreamFixture {
    #[must_use]
    pub fn from_source(source: &str) -> Self {
        let mut fixture = Self::default();
        fixture.stream.push(source);
        fixture
    }

    /// Renders the current stream through the streaming cache.
    pub fn layout(&mut self) -> MarkdownLayout {
        MarkdownRenderer::new().render_stream_layout(
            &mut self.state,
            &self.stream,
            self.options,
            &self.theme,
            &mut self.highlighter,
        )
    }

    #[must_use]
    pub fn with_options(mut self, options: MarkdownLayoutOptions) -> Self {
        self.options = options;
        self
    }

    #[must_use]
    pub fn with_theme(mut self, theme: ReviewTheme) -> Self {
        self.theme = theme;
        self
    }

    pub fn render(&mut self) -> Arc<[Line<'static>]> {
        self.layout().materialize()
    }

    pub fn render_lines(&mut self) -> Arc<[Line<'static>]> {
        MarkdownRenderer::new().render_stream_lines(
            &mut self.state,
            &self.stream,
            self.options,
            &self.theme,
            &mut self.highlighter,
        )
    }

    #[must_use]
    pub fn one_shot_lines(&self) -> Arc<[Line<'static>]> {
        MarkdownRenderer::new().render_lines(
            &MarkdownDocument::parse(self.stream.source()),
            self.options,
            &self.theme,
            &mut SyntaxHighlighter::default(),
        )
    }

    /// Renders the current source from scratch, without any streaming cache.
    #[must_use]
    pub fn one_shot(&self) -> MarkdownLayout {
        MarkdownRenderer::new().render_layout(
            &MarkdownDocument::parse(self.stream.source()),
            self.options,
            &self.theme,
            &mut SyntaxHighlighter::default(),
        )
    }

    /// Renders, then applies the row update since the host's revision to `host`.
    pub fn apply_update(&mut self) -> MarkdownRowUpdate {
        self.layout();
        let update = self.state.update_since(self.revision);
        if update.reset {
            self.host.clear();
        }
        self.host.truncate(update.first_changed_row);
        self.host.extend(update.replacement.iter().cloned());
        self.revision = update.revision;
        update
    }

    /// Panics if the streamed snapshot, the host mirror, or a one-shot render
    /// of the same source disagree.
    #[track_caller]
    pub fn assert_equivalent(&mut self) {
        self.apply_update();
        let actual = self.layout();
        let expected = self.one_shot();
        assert!(
            actual.rows().iter().eq(expected.rows().iter()),
            "source: {:?}",
            self.stream.source()
        );
        assert!(
            self.host.iter().eq(actual.rows().iter()),
            "host mirror diverged for source: {:?}",
            self.stream.source()
        );
    }
}
