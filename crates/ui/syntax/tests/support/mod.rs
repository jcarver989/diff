use clankerdiff_syntax::{DocumentHighlights, HighlightLine, SyntaxError, SyntaxStream, SyntaxStreamUpdate, analyze};
use std::{error::Error, sync::Arc};

pub type TestResult = Result<(), Box<dyn Error>>;

pub struct SyntaxBuilder {
    language: String,
}

impl Default for SyntaxBuilder {
    fn default() -> Self {
        Self { language: "rust".to_owned() }
    }
}

impl SyntaxBuilder {
    pub fn language(mut self, language: &str) -> Self {
        language.clone_into(&mut self.language);
        self
    }

    pub fn build(self) -> SyntaxFixture {
        SyntaxFixture { stream: SyntaxStream::new(self.language.as_str()), language: self.language }
    }
}

pub struct SyntaxFixture {
    pub stream: SyntaxStream,
    language: String,
}

impl SyntaxFixture {
    pub fn highlight(&self, source: &str) -> Arc<DocumentHighlights> {
        analyze(self.language.as_str(), source).highlights
    }

    pub fn append(&mut self, source: &str) -> Result<SyntaxStreamUpdate, SyntaxError> {
        self.stream.append(source)
    }

    pub fn assert_equivalent(&self) {
        let source = self.stream.source();
        let complete = analyze(self.language.as_str(), source).highlights;
        assert_eq!(self.stream.highlights().line_count(), complete.line_count());
        for index in 0..complete.line_count() {
            assert_eq!(
                self.stream.highlights().line(index),
                complete.line(index),
                "{}: {source:?}, line {index}",
                self.language
            );
        }
    }
}

pub fn line(highlights: &DocumentHighlights, index: usize) -> Result<HighlightLine<'_>, Box<dyn Error>> {
    highlights.line(index).ok_or_else(|| format!("missing line {index}").into())
}
