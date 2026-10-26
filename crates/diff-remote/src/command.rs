use crate::{Action, Invocation, Lifecycle, RemoteError, RemoteSpec};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::process::{Command, Stdio};
use std::time::SystemTime;

pub trait Remotes: Send + Sync + 'static {
    fn list(&self, remote: &RemoteSpec) -> Result<Vec<Instance>, RemoteError>;

    fn launch(&self, remote: &RemoteSpec) -> Result<String, RemoteError>;

    fn change(&self, remote: &RemoteSpec, id: &str, change: Lifecycle) -> Result<(), RemoteError>;
}

pub struct CommandRemotes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    pub id: String,
    pub state: InstanceState,
    pub expires_at: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceState {
    Starting,
    Running,
    Suspending,
    Suspended,
    Stopping,
    Ended,
}

impl Remotes for CommandRemotes {
    fn list(&self, remote: &RemoteSpec) -> Result<Vec<Instance>, RemoteError> {
        let listed: Listed = required(remote, Action::List)?.run_json()?;
        listed
            .instances
            .into_iter()
            .filter(|listed| listed.state != InstanceState::Ended)
            .map(Instance::try_from)
            .collect()
    }

    fn launch(&self, remote: &RemoteSpec) -> Result<String, RemoteError> {
        let invocation = required(remote, Action::Launch)?;
        let stdout = invocation.run()?;
        let text = String::from_utf8_lossy(&stdout);
        let text = text.trim();
        let id = if text.starts_with('{') {
            serde_json::from_str::<Launched>(text)
                .map_err(|source| RemoteError::Output {
                    program: invocation.program.clone(),
                    source,
                })?
                .id
        } else {
            text.lines().next().unwrap_or_default().trim().to_owned()
        };
        if id.is_empty() {
            return Err(RemoteError::NoId {
                program: invocation.program,
            });
        }
        Ok(id)
    }

    fn change(&self, remote: &RemoteSpec, id: &str, change: Lifecycle) -> Result<(), RemoteError> {
        let invocation = remote
            .invocation(Action::from(change), Some(id))?
            .ok_or_else(|| unsupported(remote, Action::from(change)))?;
        invocation.run().map(drop)
    }
}

impl Invocation {
    pub fn run(&self) -> Result<Vec<u8>, RemoteError> {
        let output = Command::new(&self.program)
            .args(&self.args)
            .envs(&self.env)
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| RemoteError::Spawn {
                program: self.program.clone(),
                source,
            })?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last_line = stderr.lines().map(str::trim).rfind(|line| !line.is_empty());
        Err(RemoteError::Failed(last_line.map_or_else(
            || format!("{} {}", self.program, output.status),
            str::to_owned,
        )))
    }

    pub fn run_json<T: DeserializeOwned>(&self) -> Result<T, RemoteError> {
        serde_json::from_slice(&self.run()?).map_err(|source| RemoteError::Output {
            program: self.program.clone(),
            source,
        })
    }
}

impl InstanceState {
    #[must_use]
    pub fn is_settling(self) -> bool {
        matches!(self, Self::Starting | Self::Suspending | Self::Stopping)
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "Starting",
            Self::Running => "Running",
            Self::Suspending => "Suspending",
            Self::Suspended => "Suspended",
            Self::Stopping => "Stopping",
            Self::Ended => "Ended",
        }
    }
}

#[must_use]
pub fn short_id(id: &str) -> &str {
    let start = id.char_indices().rev().nth(3).map_or(0, |(index, _)| index);
    &id[start..]
}

#[derive(Deserialize)]
struct Listed {
    instances: Vec<ListedInstance>,
}

#[derive(Deserialize)]
struct ListedInstance {
    id: String,
    state: InstanceState,
    expires_at: Option<String>,
}

#[derive(Deserialize)]
struct Launched {
    id: String,
}

impl TryFrom<ListedInstance> for Instance {
    type Error = RemoteError;

    fn try_from(listed: ListedInstance) -> Result<Self, Self::Error> {
        let expires_at = listed
            .expires_at
            .map(|time| humantime::parse_rfc3339_weak(&time).map_err(|_| RemoteError::Time(time)))
            .transpose()?;
        Ok(Self {
            id: listed.id,
            state: listed.state,
            expires_at,
        })
    }
}

fn required(remote: &RemoteSpec, action: Action) -> Result<Invocation, RemoteError> {
    remote
        .invocation(action, None)?
        .ok_or_else(|| unsupported(remote, action))
}

fn unsupported(remote: &RemoteSpec, action: Action) -> RemoteError {
    RemoteError::Unsupported {
        remote: remote.name.clone(),
        action,
    }
}
