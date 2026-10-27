use serde::{Deserialize, Serialize};

const TRAILER_FENCE: &str = "---";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommitKind {
    Feature,
    Fix,
    Refactor,
    Performance,
    Docs,
    Test,
    Build,
    Ci,
    Chore,
    Style,
    Revert,
}

impl CommitKind {
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_lowercase().as_str() {
            "feat" | "feature" => Self::Feature,
            "fix" => Self::Fix,
            "refactor" => Self::Refactor,
            "perf" => Self::Performance,
            "docs" => Self::Docs,
            "test" | "tests" => Self::Test,
            "build" => Self::Build,
            "ci" => Self::Ci,
            "chore" => Self::Chore,
            "style" => Self::Style,
            "revert" => Self::Revert,
            _ => return None,
        })
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Feature => "Feature",
            Self::Fix => "Fix",
            Self::Refactor => "Refactor",
            Self::Performance => "Performance",
            Self::Docs => "Docs",
            Self::Test => "Tests",
            Self::Build => "Build",
            Self::Ci => "CI",
            Self::Chore => "Chore",
            Self::Style => "Style",
            Self::Revert => "Revert",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitTitle {
    pub kind: Option<CommitKind>,
    pub scope: Option<String>,
    pub summary: String,
    pub pull_request: Option<u64>,
}

impl CommitTitle {
    #[must_use]
    pub fn parse(subject: &str) -> Self {
        let (subject, pull_request) = split_pull_request(subject.trim());
        match split_conventional(subject) {
            Some((kind, scope, rest)) => {
                Self { kind: Some(kind), scope: scope.map(str::to_owned), summary: capitalized(rest), pull_request }
            }
            None => Self { kind: None, scope: None, summary: subject.to_owned(), pull_request },
        }
    }
}

#[must_use]
pub fn commit_description(body: &str) -> String {
    let body = body.lines().take_while(|line| line.trim() != TRAILER_FENCE).collect::<Vec<_>>().join("\n");
    let mut paragraphs: Vec<&str> =
        body.split("\n\n").map(str::trim).filter(|paragraph| !paragraph.is_empty()).collect();
    if paragraphs.last().is_some_and(|paragraph| paragraph.lines().all(is_trailer)) {
        paragraphs.pop();
    }
    paragraphs.join("\n\n")
}

fn split_pull_request(subject: &str) -> (&str, Option<u64>) {
    subject
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once("(#"))
        .and_then(|(rest, number)| Some((rest.trim_end(), number.parse().ok()?)))
        .map_or((subject, None), |(rest, number)| (rest, Some(number)))
}

fn split_conventional(subject: &str) -> Option<(CommitKind, Option<&str>, &str)> {
    let (head, rest) = subject.split_once(':')?;
    let rest = rest.strip_prefix(' ')?.trim_start();
    let head = head.strip_suffix('!').unwrap_or(head);
    let (word, scope) = match head.split_once('(') {
        Some((word, scope)) => (word, Some(scope.strip_suffix(')')?).filter(|scope| !scope.is_empty())),
        None => (head, None),
    };
    if rest.is_empty() || !word.chars().all(|character| character.is_ascii_alphabetic()) {
        return None;
    }
    Some((CommitKind::parse(word)?, scope, rest))
}

fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| first.to_uppercase().chain(characters).collect())
}

fn is_trailer(line: &str) -> bool {
    line.split_once(": ").is_some_and(|(key, value)| {
        !value.is_empty()
            && key.contains('-')
            && key.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')
    })
}
