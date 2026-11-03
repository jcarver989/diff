use crate::{LanguageHint, SourceSyntax, analyze};
use std::{
    fmt,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Default)]
pub struct SyntaxCell(Arc<OnceLock<SourceSyntax>>);

impl SyntaxCell {
    pub fn get<'a, T: AsRef<str>>(
        &self,
        language: impl Into<LanguageHint<'a>>,
        source: impl FnOnce() -> T,
    ) -> &SourceSyntax {
        self.0.get_or_init(|| analyze(language, source().as_ref()))
    }
}

impl From<SourceSyntax> for SyntaxCell {
    fn from(syntax: SourceSyntax) -> Self {
        Self(Arc::new(OnceLock::from(syntax)))
    }
}

impl PartialEq for SyntaxCell {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for SyntaxCell {}

impl fmt::Debug for SyntaxCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SyntaxCell")
    }
}
