use thiserror::Error;
use tree_sitter::LanguageError;

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("syntax input requires {attempted} bytes, exceeding the {limit}-byte limit")]
    InputLimit { limit: usize, attempted: usize },
    #[error("missing syntax grammar: {language}")]
    MissingGrammar { language: String },
    #[error("failed to configure the syntax parser: {0}")]
    Language(#[from] LanguageError),
    #[error("syntax parser returned no tree")]
    NoTree,
}
