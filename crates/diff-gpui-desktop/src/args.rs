use diff_core::{DiffScope, ParseDiffScopeError};
use std::{ffi::OsString, path::PathBuf};
use thiserror::Error;

/// Desktop command-line usage text.
pub const USAGE: &str = "Usage: diff-gpui-desktop [OPTIONS] [REPOSITORY]\n\nOptions:\n  -s, --scope <SCOPE>  Initial scope: unstaged, staged, or both [default: both]\n      --unstaged       Show unstaged changes\n      --staged         Show staged changes\n      --both           Show all working-tree changes\n  -h, --help           Print help";

/// Arguments accepted by the desktop host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    /// Repository or a path contained by it.
    pub repository: PathBuf,
    /// Initial Git diff scope.
    pub scope: DiffScope,
}

impl CliArgs {
    /// Parses arguments from the current process.
    ///
    /// # Errors
    ///
    /// Returns [`ArgsError`] when an argument is invalid or help was requested.
    pub fn parse() -> Result<Self, ArgsError> {
        Self::parse_from(std::env::args_os().skip(1))
    }

    /// Parses a supplied argument sequence.
    ///
    /// # Errors
    ///
    /// Returns [`ArgsError`] when an argument is invalid or help was requested.
    pub fn parse_from(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, ArgsError> {
        let mut arguments = arguments.into_iter();
        let mut repository = None;
        let mut scope = DiffScope::Both;
        while let Some(argument) = arguments.next() {
            let text = argument.to_string_lossy();
            match text.as_ref() {
                "-h" | "--help" => return Err(ArgsError::Help),
                "--unstaged" => scope = DiffScope::Unstaged,
                "--staged" => scope = DiffScope::Staged,
                "--both" => scope = DiffScope::Both,
                "-s" | "--scope" => {
                    let value = arguments.next().ok_or(ArgsError::MissingScope)?;
                    scope = value.to_string_lossy().parse()?;
                }
                value if value.starts_with("--scope=") => {
                    scope = value["--scope=".len()..].parse()?;
                }
                value if value.starts_with('-') => {
                    return Err(ArgsError::UnknownOption(value.to_owned()));
                }
                _ if repository.is_some() => return Err(ArgsError::MultipleRepositories),
                _ => repository = Some(PathBuf::from(argument)),
            }
        }
        let repository = match repository {
            Some(repository) => repository,
            None => std::env::current_dir().map_err(ArgsError::CurrentDirectory)?,
        };
        Ok(Self { repository, scope })
    }
}

/// An invalid desktop command-line invocation.
#[derive(Debug, Error)]
pub enum ArgsError {
    #[error("help requested")]
    Help,
    #[error("--scope requires unstaged, staged, or both")]
    MissingScope,
    #[error(transparent)]
    InvalidScope(#[from] ParseDiffScopeError),
    #[error("unknown option `{0}`")]
    UnknownOption(String),
    #[error("only one repository path may be supplied")]
    MultipleRepositories,
    #[error("could not determine the current directory: {0}")]
    CurrentDirectory(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repository_and_scope() {
        let args = CliArgs::parse_from(["--scope", "staged", "/tmp/repo"].map(OsString::from))
            .expect("arguments should parse");
        assert_eq!(args.repository, PathBuf::from("/tmp/repo"));
        assert_eq!(args.scope, DiffScope::Staged);
    }

    #[test]
    fn supports_scope_shorthand() {
        let args = CliArgs::parse_from(["--unstaged", "."].map(OsString::from))
            .expect("arguments should parse");
        assert_eq!(args.scope, DiffScope::Unstaged);
    }

    #[test]
    fn rejects_unknown_scope() {
        let error = CliArgs::parse_from(["--scope=other"].map(OsString::from))
            .expect_err("scope should be rejected");
        assert!(matches!(error, ArgsError::InvalidScope(_)));
    }
}
