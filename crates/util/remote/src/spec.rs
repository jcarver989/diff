use crate::RemoteError;
use crate::vars::expand;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemotesFile {
    #[serde(default)]
    pub remotes: Vec<RemoteSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteSpec {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspend: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acp: Option<Acp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AcpFields", into = "AcpFields")]
pub struct Acp {
    pub transport: AcpCommand,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpCommand {
    Stdio(Vec<String>),
    Websocket(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    List,
    Launch,
    Suspend,
    Resume,
    Stop,
    Shell,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Suspend,
    Resume,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub dir: PathBuf,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpTarget {
    pub transport: AcpTransport,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpTransport {
    Stdio(Invocation),
    Websocket(Invocation),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcpFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stdio: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    websocket: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

pub const REMOTES_FILE: &str = "remotes.json";

#[must_use]
pub fn default_file() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join(".config").join("clanker").join(REMOTES_FILE))
}

impl RemotesFile {
    pub fn read(file: &Path) -> Result<Self, RemoteError> {
        match fs::read_to_string(file) {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|source| RemoteError::Parse { path: file.to_owned(), source })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(RemoteError::Read { path: file.to_owned(), source }),
        }
    }
}

impl RemoteSpec {
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    #[must_use]
    pub fn is_static(&self) -> bool {
        self.list.is_none()
    }

    #[must_use]
    pub fn can(&self, action: Action) -> bool {
        self.command(action).is_some()
    }

    #[must_use]
    pub fn can_change(&self, change: Lifecycle) -> bool {
        self.can(Action::from(change))
    }

    pub fn invocation(&self, action: Action, instance: Option<&str>) -> Result<Option<Invocation>, RemoteError> {
        self.command(action).map(|argv| self.expand_command(argv, action, instance)).transpose()
    }

    pub fn acp(&self, instance: Option<&str>) -> Result<Option<AcpTarget>, RemoteError> {
        let Some(acp) = &self.acp else {
            return Ok(None);
        };
        let transport = match &acp.transport {
            AcpCommand::Stdio(argv) => AcpTransport::Stdio(self.expand_command(argv, "acp", instance)?),
            AcpCommand::Websocket(argv) => AcpTransport::Websocket(self.expand_command(argv, "acp", instance)?),
        };
        let cwd = acp.cwd.as_deref().map(|cwd| self.expand(cwd, instance)).transpose()?;
        Ok(Some(AcpTarget { transport, cwd }))
    }

    fn command(&self, action: Action) -> Option<&Vec<String>> {
        match action {
            Action::List => self.list.as_ref(),
            Action::Launch => self.launch.as_ref(),
            Action::Suspend => self.suspend.as_ref(),
            Action::Resume => self.resume.as_ref(),
            Action::Stop => self.stop.as_ref(),
            Action::Shell => self.shell.as_ref(),
            Action::Logs => self.logs.as_ref(),
        }
    }

    fn expand_command(
        &self,
        words: &[String],
        action: impl fmt::Display,
        instance: Option<&str>,
    ) -> Result<Invocation, RemoteError> {
        let Some((program, rest)) = words.split_first() else {
            return Err(RemoteError::EmptyCommand { remote: self.name.clone(), action: action.to_string() });
        };
        let program = expand_home(Path::new(&self.expand(program, instance)?)).to_string_lossy().into_owned();
        let args = rest.iter().map(|arg| self.expand(arg, instance)).collect::<Result<_, _>>()?;
        let env = self
            .env
            .iter()
            .map(|(name, value)| Ok((name.clone(), self.expand(value, instance)?)))
            .collect::<Result<_, RemoteError>>()?;
        let dir = self.dir.as_deref().map(expand_home).or_else(std::env::home_dir).unwrap_or_else(|| "/".into());
        Ok(Invocation { program, args, dir, env })
    }

    fn expand(&self, template: &str, instance: Option<&str>) -> Result<String, RemoteError> {
        expand(template, |name| match name {
            "ID" => instance.map(str::to_owned),
            "NAME" => Some(self.name.clone()),
            _ => self.vars.get(name).cloned().or_else(|| std::env::var(name).ok()),
        })
    }
}

impl Acp {
    #[must_use]
    pub fn stdio(argv: Vec<String>) -> Self {
        Self { transport: AcpCommand::Stdio(argv), cwd: None }
    }

    #[must_use]
    pub fn websocket(argv: Vec<String>) -> Self {
        Self { transport: AcpCommand::Websocket(argv), cwd: None }
    }
}

impl TryFrom<AcpFields> for Acp {
    type Error = &'static str;

    fn try_from(fields: AcpFields) -> Result<Self, Self::Error> {
        let transport = match (fields.stdio, fields.websocket) {
            (Some(argv), None) => AcpCommand::Stdio(argv),
            (None, Some(argv)) => AcpCommand::Websocket(argv),
            _ => return Err("`acp` needs exactly one of `stdio` and `websocket`"),
        };
        Ok(Self { transport, cwd: fields.cwd })
    }
}

impl From<Acp> for AcpFields {
    fn from(acp: Acp) -> Self {
        let (stdio, websocket) = match acp.transport {
            AcpCommand::Stdio(argv) => (Some(argv), None),
            AcpCommand::Websocket(argv) => (None, Some(argv)),
        };
        Self { stdio, websocket, cwd: acp.cwd }
    }
}

impl From<Lifecycle> for Action {
    fn from(change: Lifecycle) -> Self {
        match change {
            Lifecycle::Suspend => Self::Suspend,
            Lifecycle::Resume => Self::Resume,
            Lifecycle::Stop => Self::Stop,
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::List => "list",
            Self::Launch => "launch",
            Self::Suspend => "suspend",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::Shell => "shell",
            Self::Logs => "logs",
        })
    }
}

fn expand_home(path: &Path) -> PathBuf {
    match (path.strip_prefix("~"), std::env::home_dir()) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path.to_owned(),
    }
}
