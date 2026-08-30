//! Stable, versioned JSON envelopes emitted by the `clankerdiff` executable.
//!
//! Protocol compatibility is independent from this crate's package version.
//! Deserialization accepts unknown fields for forward compatibility, while
//! [`parse_response`] validates versions and outcome/submission invariants.

use diff_core::{DiffScope, ReviewSubmission};
use diff_markdown::MarkdownReviewSubmission;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReviewOutcome {
    Approved,
    ChangesRequested,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "document_kind", rename_all = "snake_case")]
pub enum ReviewResponse {
    Diff {
        protocol_version: u32,
        outcome: ReviewOutcome,
        repository_root: PathBuf,
        #[serde(with = "diff_scope")]
        scope: DiffScope,
        #[serde(skip_serializing_if = "Option::is_none")]
        submission: Option<ReviewSubmission>,
    },
    Markdown {
        protocol_version: u32,
        outcome: ReviewOutcome,
        source_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        submission: Option<MarkdownReviewSubmission>,
    },
}

impl ReviewResponse {
    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        match self {
            Self::Diff {
                protocol_version, ..
            }
            | Self::Markdown {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    #[must_use]
    pub const fn outcome(&self) -> ReviewOutcome {
        match self {
            Self::Diff { outcome, .. } | Self::Markdown { outcome, .. } => *outcome,
        }
    }

    /// Checks protocol version and outcome/submission invariants.
    ///
    /// # Errors
    /// Returns an error for an unsupported version or inconsistent submission.
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        if self.protocol_version() != PROTOCOL_VERSION {
            return Err(ProtocolValidationError::UnsupportedVersion {
                received: self.protocol_version(),
                supported: PROTOCOL_VERSION,
            });
        }
        let has_submission = match self {
            Self::Diff { submission, .. } => submission.is_some(),
            Self::Markdown { submission, .. } => submission.is_some(),
        };
        match (self.outcome(), has_submission) {
            (ReviewOutcome::Cancelled, false)
            | (ReviewOutcome::Approved | ReviewOutcome::ChangesRequested, true) => Ok(()),
            (ReviewOutcome::Cancelled, true) => {
                Err(ProtocolValidationError::CancellationHasSubmission)
            }
            (ReviewOutcome::Approved | ReviewOutcome::ChangesRequested, false) => {
                Err(ProtocolValidationError::SubmittedWithoutSubmission)
            }
        }
    }
}

mod diff_scope {
    use diff_core::DiffScope;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};
    use std::str::FromStr;

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde with modules require this serializer signature"
    )]
    pub fn serialize<S>(scope: &DiffScope, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(scope.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DiffScope, D::Error>
    where
        D: Deserializer<'de>,
    {
        let scope = String::deserialize(deserializer)?;
        DiffScope::from_str(&scope).map_err(D::Error::custom)
    }
}

/// Parses and validates a subprocess review response.
///
/// # Errors
/// Returns an error for malformed JSON, unsupported protocol versions, or an
/// invalid outcome/submission combination.
pub fn parse_response(bytes: &[u8]) -> Result<ReviewResponse, ParseResponseError> {
    let response = serde_json::from_slice::<ReviewResponse>(bytes)?;
    response.validate()?;
    Ok(response)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityResponse {
    pub protocol_version: u32,
    pub supported_protocol_versions: Vec<u32>,
    pub review_kinds: Vec<ReviewKind>,
    pub uis: Vec<UiKind>,
    pub current_terminal_tui: bool,
}

impl Default for CapabilityResponse {
    fn default() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            supported_protocol_versions: vec![PROTOCOL_VERSION],
            review_kinds: vec![ReviewKind::Diff, ReviewKind::Markdown],
            uis: vec![UiKind::Tui, UiKind::Desktop],
            current_terminal_tui: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewKind {
    Diff,
    Markdown,
}

impl ReviewKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Markdown => "markdown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiKind {
    Tui,
    Desktop,
}

impl UiKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Desktop => "desktop",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolValidationError {
    #[error(
        "unsupported Clankerdiff protocol version {received}; this client supports version {supported}"
    )]
    UnsupportedVersion { received: u32, supported: u32 },
    #[error("a cancelled review must not contain a submission")]
    CancellationHasSubmission,
    #[error("an approved or changes-requested review must contain a submission")]
    SubmittedWithoutSubmission,
}

#[derive(Debug, Error)]
pub enum ParseResponseError {
    #[error("invalid Clankerdiff response JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Validation(#[from] ProtocolValidationError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_submitted_and_cancelled_responses() {
        let responses = [
            ReviewResponse::Diff {
                protocol_version: PROTOCOL_VERSION,
                outcome: ReviewOutcome::Approved,
                repository_root: PathBuf::from("/repo"),
                scope: DiffScope::Both,
                submission: Some(ReviewSubmission {
                    comments: Vec::new(),
                    formatted: "approved".into(),
                }),
            },
            ReviewResponse::Markdown {
                protocol_version: PROTOCOL_VERSION,
                outcome: ReviewOutcome::Cancelled,
                source_path: Some("plans/a.md".into()),
                submission: None,
            },
        ];
        for response in &responses {
            let json = serde_json::to_vec(response).unwrap();
            let parsed = parse_response(&json).unwrap();
            assert_eq!(&parsed, response);
        }

        let json = serde_json::to_string(&responses[0]).unwrap();
        assert!(json.contains(r#""scope":"both""#));
    }

    #[test]
    fn rejects_invalid_outcome_submission_combinations() {
        let missing = ReviewResponse::Diff {
            protocol_version: PROTOCOL_VERSION,
            outcome: ReviewOutcome::Approved,
            repository_root: PathBuf::from("/repo"),
            scope: DiffScope::Both,
            submission: None,
        };
        assert_eq!(
            missing.validate(),
            Err(ProtocolValidationError::SubmittedWithoutSubmission)
        );
    }

    #[test]
    fn accepts_unknown_fields_and_rejects_new_versions() {
        let cancelled = br#"{
            "document_kind":"markdown",
            "protocol_version":1,
            "outcome":"cancelled",
            "source_path":"plans/a.md",
            "future_field":true
        }"#;
        let response = parse_response(cancelled).unwrap();
        assert_eq!(response.outcome(), ReviewOutcome::Cancelled);

        let newer = cancelled.to_vec();
        let newer = String::from_utf8(newer).unwrap().replacen(":1", ":2", 1);
        assert!(matches!(
            parse_response(newer.as_bytes()),
            Err(ParseResponseError::Validation(
                ProtocolValidationError::UnsupportedVersion { received: 2, .. }
            ))
        ));
    }
}
