use arborium::{Config, Highlighter};
use arborium_highlight::spans_to_flat_tokens;
use arborium_theme::tag_to_name;
use clankerdiff_syntax::{
    CacheConfig, DocumentHighlights, Fingerprint, HighlightSpan, SyntaxError, SyntaxHighlighter,
    SyntaxStream, SyntaxStreamUpdate, SyntaxTheme, resolve_language,
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
            reference: Highlighter::with_config(Config {
                max_injection_depth: 3,
                ..Config::default()
            }),
        }
    }
}

pub struct SyntaxFixture {
    pub highlighter: SyntaxHighlighter,
    pub stream: SyntaxStream,
    pub theme: SyntaxTheme,
    language: String,
    reference: Highlighter,
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
        let expected = reference_lines(&mut self.reference, &self.theme, &self.language, source)?;
        assert_eq!(self.stream.highlights().line_count(), expected.len());
        for (index, line) in expected.iter().enumerate() {
            assert_eq!(
                self.stream.highlights().line(index),
                Some(line.as_slice()),
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

fn reference_lines(
    highlighter: &mut Highlighter,
    theme: &SyntaxTheme,
    hint: &str,
    source: &str,
) -> Result<Vec<Vec<HighlightSpan>>, Box<dyn Error>> {
    let raw = resolve_language(hint, source)
        .map(|language| highlighter.highlight_spans(language, source))
        .transpose()?
        .unwrap_or_default();
    let spans: Vec<_> = spans_to_flat_tokens(source, raw)
        .into_iter()
        .filter_map(|token| {
            let name = match tag_to_name(token.tag)? {
                "title" => "markup.heading",
                "strong" => "markup.bold",
                "emphasis" => "markup.italic",
                "link" => "markup.link",
                "literal" => "markup.raw",
                "strikethrough" => "markup.strikethrough",
                name => name,
            };
            let style = theme.style(name)?;
            Some(HighlightSpan {
                range: token.start as usize..token.end as usize,
                foreground: style.foreground,
                font_style: style.font_style,
            })
        })
        .collect();
    let mut offset = 0;
    Ok(source
        .split_terminator('\n')
        .map(|text| {
            let end = offset + text.strip_suffix('\r').unwrap_or(text).len();
            let mut line: Vec<HighlightSpan> = Vec::new();
            for span in &spans {
                let start = span.range.start.max(offset);
                let to = span.range.end.min(end);
                if start >= to {
                    continue;
                }
                let span = HighlightSpan {
                    range: start - offset..to - offset,
                    foreground: span.foreground,
                    font_style: span.font_style,
                };
                if let Some(last) = line.last_mut()
                    && last.range.end == span.range.start
                    && last.foreground == span.foreground
                    && last.font_style == span.font_style
                {
                    last.range.end = span.range.end;
                } else {
                    line.push(span);
                }
            }
            offset += text.len() + 1;
            line
        })
        .collect())
}
