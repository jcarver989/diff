#![doc = include_str!("../README.md")]

mod client;
#[cfg(feature = "daemon")]
mod daemon;
#[cfg(feature = "daemon")]
mod diagnostics_store;
#[cfg(feature = "daemon")]
mod file_watcher;
#[cfg(feature = "daemon")]
mod hooks;
mod language_catalog;
#[cfg(feature = "daemon")]
mod language_server;
#[cfg(feature = "daemon")]
mod log_file;
mod pending_requests;
mod protocol;
#[cfg(feature = "test-support")]
pub mod testing;
mod uri;
#[cfg(feature = "daemon")]
mod workspace;
mod workspace_key;
#[cfg(feature = "daemon")]
mod workspace_session;

pub use client::{ClientError, LspClient};
#[cfg(feature = "daemon")]
pub use daemon::{DaemonError, run_if_requested};
pub use language_catalog::{LanguageId, detect_project_languages};
pub use uri::{UriError, path_to_uri, uri_to_path};
pub use workspace_key::WorkspaceKey;
