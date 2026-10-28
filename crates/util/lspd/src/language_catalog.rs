use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum LanguageId {
    Rust,
    TypeScript,
    TypeScriptReact,
    JavaScript,
    JavaScriptReact,
    Python,
    Go,
    Cpp,
    C,
}

impl LanguageId {
    pub const ALL: &[Self] = &[
        Self::Rust,
        Self::TypeScript,
        Self::TypeScriptReact,
        Self::JavaScript,
        Self::JavaScriptReact,
        Self::Python,
        Self::Go,
        Self::Cpp,
        Self::C,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::TypeScriptReact => "typescriptreact",
            Self::JavaScript => "javascript",
            Self::JavaScriptReact => "javascriptreact",
            Self::Python => "python",
            Self::Go => "go",
            Self::Cpp => "cpp",
            Self::C => "c",
        }
    }

    #[must_use]
    pub(crate) const fn server(self) -> &'static ServerSpec {
        match self {
            Self::Rust => &RUST_ANALYZER,
            Self::TypeScript | Self::TypeScriptReact | Self::JavaScript | Self::JavaScriptReact => &TSC,
            Self::Python => &PYRIGHT,
            Self::Go => &GOPLS,
            Self::Cpp | Self::C => &CLANGD,
        }
    }

    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        Self::ALL.iter().copied().find(|language| language.extensions().contains(&extension))
    }

    const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::TypeScript => &["ts"],
            Self::TypeScriptReact => &["tsx"],
            Self::JavaScript => &["js", "mjs"],
            Self::JavaScriptReact => &["jsx"],
            Self::Python => &["py", "pyi", "pyw"],
            Self::Go => &["go"],
            Self::Cpp => &["cpp", "cxx", "cc", "hpp", "hxx", "hh"],
            Self::C => &["c", "h"],
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct ServerSpec {
    pub(crate) name: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) installation_instructions: Option<&'static str>,
    args: &'static [&'static str],
    pub(crate) diagnostics_mode: DiagnosticsMode,
    pub(crate) reports_server_status: bool,
    pub(crate) project: ProjectMarkers,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct ProjectMarkers {
    pub(crate) workspace: Option<(&'static str, &'static str)>,
    pub(crate) projects: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DiagnosticsMode {
    Push,
    Pull,
}

#[cfg(feature = "daemon")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerCommand {
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
}

#[cfg(feature = "daemon")]
impl ServerSpec {
    pub(crate) fn serves(&self, path: &Path) -> bool {
        LanguageId::from_path(path).is_some_and(|language| language.server() == self)
    }

    pub(crate) fn command(&self) -> ServerCommand {
        ServerCommand { command: self.name.to_owned(), args: self.args.iter().map(ToString::to_string).collect() }
    }

    pub(crate) fn unavailable(&self, error: &impl ToString) -> String {
        let error = error.to_string();
        match self.installation_instructions {
            Some(instructions) => format!("{} could not start: {error}. {instructions}", self.display_name),
            None => format!("{} could not start: {error}", self.display_name),
        }
    }
}

#[must_use]
pub fn detect_project_languages(root: &Path) -> Vec<LanguageId> {
    let mut detected: Vec<LanguageId> = Vec::new();
    for &language in LanguageId::ALL {
        let project = &language.server().project;
        let mut markers =
            project.workspace.map(|(manifest, _)| manifest).into_iter().chain(project.projects.iter().copied());
        if !detected.iter().any(|other| other.server() == language.server())
            && markers.any(|marker| root.join(marker).is_file())
        {
            detected.push(language);
        }
    }
    detected
}

const PUSH: ServerSpec = ServerSpec {
    name: "",
    display_name: "",
    installation_instructions: None,
    args: &[],
    diagnostics_mode: DiagnosticsMode::Push,
    reports_server_status: false,
    project: ProjectMarkers { workspace: None, projects: &[] },
};

const RUST_ANALYZER: ServerSpec = ServerSpec {
    name: "rust-analyzer",
    display_name: "rust-analyzer",
    reports_server_status: true,
    project: ProjectMarkers { workspace: Some(("Cargo.toml", "[workspace]")), projects: &["Cargo.toml"] },
    ..PUSH
};

const TSC: ServerSpec = ServerSpec {
    name: "tsc",
    display_name: "TypeScript 7 language server",
    installation_instructions: Some(
        "It requires TypeScript 7 or newer. Install it in this workspace with \
         `npm install --save-dev typescript@^7`, or install it globally with \
         `npm install --global typescript@^7`, then retry.",
    ),
    args: &["--lsp", "--stdio"],
    diagnostics_mode: DiagnosticsMode::Pull,
    project: ProjectMarkers {
        workspace: Some(("package.json", "\"workspaces\"")),
        projects: &["package.json", "tsconfig.json"],
    },
    ..PUSH
};

const PYRIGHT: ServerSpec = ServerSpec {
    name: "pyright-langserver",
    display_name: "Pyright language server",
    args: &["--stdio"],
    project: ProjectMarkers {
        workspace: None,
        projects: &["pyproject.toml", "setup.cfg", "setup.py", "requirements.txt"],
    },
    ..PUSH
};

const GOPLS: ServerSpec = ServerSpec {
    name: "gopls",
    display_name: "gopls language server",
    project: ProjectMarkers { workspace: Some(("go.work", "")), projects: &["go.mod"] },
    ..PUSH
};

const CLANGD: ServerSpec = ServerSpec {
    name: "clangd",
    display_name: "clangd language server",
    project: ProjectMarkers { workspace: Some(("CMakeLists.txt", "")), projects: &[] },
    ..PUSH
};
