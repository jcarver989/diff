use crate::language_catalog::{LanguageId, ProjectMarkers, ServerSpec};
use crate::protocol::PROTOCOL_VERSION;
use sha2::{Digest, Sha256};
use std::env;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceKey {
    root: PathBuf,
    server: &'static ServerSpec,
}

impl WorkspaceKey {
    #[must_use]
    pub fn resolve(path: &Path, language: LanguageId) -> Self {
        let server = language.server();
        Self { root: workspace_root(path, &server.project), server }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn server(&self) -> &'static ServerSpec {
        self.server
    }

    pub(crate) fn socket_path(&self) -> PathBuf {
        let directory = env::var_os("XDG_RUNTIME_DIR").map_or_else(
            || PathBuf::from(format!("/tmp/clankerdiff-lspd-{}", nix::unistd::getuid().as_raw())),
            |runtime| PathBuf::from(runtime).join("clankerdiff-lspd"),
        );
        let digest = Sha256::digest(self.root.as_os_str().as_encoded_bytes());
        let hash = digest[..8].iter().fold(0_u64, |hash, byte| hash << 8 | u64::from(*byte));
        directory.join(format!("lsp-{}-v{PROTOCOL_VERSION}-{hash:016x}.sock", self.server.name))
    }
}

#[cfg(feature = "daemon")]
impl WorkspaceKey {
    pub(crate) fn trusted(root: &Path, server: &str) -> Option<Self> {
        let server = LanguageId::ALL.iter().map(|language| language.server()).find(|spec| spec.name == server)?;
        Some(Self { root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()), server })
    }
}

fn workspace_root(path: &Path, project: &ProjectMarkers) -> PathBuf {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let start = if path.is_file() { path.parent().map_or_else(|| path.clone(), Path::to_path_buf) } else { path };
    let repository = start.ancestors().find(|dir| dir.join(".git").exists()).map(Path::to_path_buf);
    let ancestors: Vec<&Path> = match &repository {
        Some(repository) => start.ancestors().take_while(|ancestor| ancestor.starts_with(repository)).collect(),
        None => start.ancestors().collect(),
    };
    let workspace = project.workspace.and_then(|(manifest, needle)| {
        ancestors.iter().rev().find(|dir| read_to_string(dir.join(manifest)).is_ok_and(|text| text.contains(needle)))
    });
    let nearest = || ancestors.iter().find(|dir| project.projects.iter().any(|marker| dir.join(marker).is_file()));
    workspace.or_else(nearest).map(|dir| dir.to_path_buf()).or(repository).unwrap_or(start)
}
