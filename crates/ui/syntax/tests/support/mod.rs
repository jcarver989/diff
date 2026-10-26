use clankerdiff_syntax::{
    CacheConfig, DocumentHighlights, Fingerprint, HighlightSpan, SyntaxError, SyntaxHighlighter,
    SyntaxStream, SyntaxStreamUpdate, SyntaxTheme,
};
use std::{error::Error, sync::Arc};

pub type TestResult = Result<(), Box<dyn Error>>;

pub struct CacheBuilder {
    config: CacheConfig,
    language: String,
    theme: SyntaxTheme,
}

impl Default for CacheBuilder {
    fn default() -> Self {
        Self {
            config: CacheConfig::default(),
            language: "rust".to_owned(),
            theme: SyntaxTheme::default(),
        }
    }
}

impl CacheBuilder {
    pub fn entries(mut self, limit: usize) -> Self {
        self.config.max_documents = limit;
        self
    }

    pub fn source_limit(mut self, limit: usize) -> Self {
        self.config.max_source_bytes = limit;
        self
    }

    pub fn language(mut self, language: &str) -> Self {
        language.clone_into(&mut self.language);
        self
    }

    pub fn build(self) -> SyntaxFixture {
        SyntaxFixture {
            highlighter: SyntaxHighlighter::new(self.config),
            stream: SyntaxStream::new(self.language.as_str()),
            theme: self.theme,
            language: self.language,
        }
    }
}

pub struct SyntaxFixture {
    pub highlighter: SyntaxHighlighter,
    pub stream: SyntaxStream,
    pub theme: SyntaxTheme,
    language: String,
}

impl SyntaxFixture {
    pub fn highlight(&mut self, source: &str) -> Result<Arc<DocumentHighlights>, SyntaxError> {
        self.highlighter.with_theme(&self.theme).highlight_document(
            Fingerprint::of([source]),
            self.language.as_str(),
            || source,
        )
    }

    pub fn append(&mut self, source: &str) -> Result<SyntaxStreamUpdate, SyntaxError> {
        self.highlighter
            .with_theme(&self.theme)
            .append(&mut self.stream, source)
    }

    pub fn assert_equivalent(&mut self) -> TestResult {
        let source = self.stream.source();
        let complete = SyntaxHighlighter::new(0)
            .with_theme(&self.theme)
            .highlight_document(Fingerprint::of([source]), self.language.as_str(), || source)?;
        assert_eq!(self.stream.highlights().line_count(), complete.line_count());
        for index in 0..complete.line_count() {
            assert_eq!(
                self.stream.highlights().line(index),
                complete.line(index),
                "{}: {source:?}, line {index}",
                self.language
            );
        }
        Ok(())
    }
}

pub fn line(
    highlights: &DocumentHighlights,
    index: usize,
) -> Result<&[HighlightSpan], Box<dyn Error>> {
    highlights
        .line(index)
        .ok_or_else(|| format!("missing line {index}").into())
}
