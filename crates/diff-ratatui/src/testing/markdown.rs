use crate::{MarkdownRenderOptions, MarkdownRenderer, StreamingMarkdownState};
use clankerdiff_markdown::{MarkdownDocument, MarkdownStream};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::ReviewTheme;
use ratatui::text::Line;
use std::sync::Arc;

#[derive(Default)]
pub struct MarkdownStreamFixture {
    pub stream: MarkdownStream,
    pub state: StreamingMarkdownState,
    pub options: MarkdownRenderOptions,
    pub theme: ReviewTheme,
    pub highlighter: SyntaxHighlighter,
}

impl MarkdownStreamFixture {
    pub fn render(&mut self) -> Arc<[Line<'static>]> {
        MarkdownRenderer::new().render_stream_lines(
            &mut self.state,
            &self.stream,
            self.options,
            &self.theme,
            &mut self.highlighter,
        )
    }

    /// Panics if the streamed snapshot differs from a one-shot render of the same source.
    #[track_caller]
    pub fn assert_equivalent(&mut self) {
        let actual = self.render();
        let expected = MarkdownRenderer::new().render_lines(
            &MarkdownDocument::parse(self.stream.source()),
            self.options,
            &self.theme,
            &mut SyntaxHighlighter::default(),
        );
        assert_eq!(actual, expected, "source: {:?}", self.stream.source());
    }
}
