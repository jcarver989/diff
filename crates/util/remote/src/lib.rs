mod command;
mod inventory;
mod spec;
#[cfg(feature = "test-support")]
pub mod testing;
mod vars;

pub use command::{CommandRemotes, Instance, InstanceState, Remotes, short_id};
pub use inventory::{Inventory, Listed, Listing, POLL, POLL_SETTLING};
pub use spec::{
    Acp, AcpTarget, Action, Channel, Invocation, Lifecycle, REMOTES_FILE, RemoteSpec, RemotesFile, default_file,
};

use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RemoteError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not a remotes file: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("${{{0}}} is not set")]
    Variable(String),
    #[error("the `{action}` command of {remote} is empty")]
    EmptyCommand { remote: String, action: String },
    #[error("{remote} has no `{action}` command")]
    Unsupported { remote: String, action: Action },
    #[error("could not run {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("{0}")]
    Failed(String),
    #[error("{program} printed something unexpected: {source}")]
    Output {
        program: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{program} printed no instance id")]
    NoId { program: String },
    #[error("`{0}` is not an RFC 3339 time")]
    Time(String),
}
