use framer::github::Step;
use framer::{KeyPath, Preset, Project, TomlKeys};

pub const ROUGE_LINUX_PACKAGES: [&str; 3] = ["libfontconfig1-dev", "libxkbcommon-dev", "libwayland-dev"];
const ROUGE_CRATES: [&str; 7] = [
    "rouge",
    "rouge-core",
    "rouge-reactive",
    "rouge-widgets",
    "rouge-theme",
    "rouge-syntax",
    "rouge-markdown",
];

pub const CRATES: [(&str, &str); 16] = [
    ("clankerdiff-fingerprint", "crates/util/fingerprint"),
    ("clankerdiff-remote", "crates/util/remote"),
    ("clankerdiff-lspd", "crates/util/lspd"),
    ("clankerdiff-ui-core", "crates/ui/core"),
    ("clankerdiff-core", "crates/diff/core"),
    ("clankerdiff-protocol", "crates/diff/protocol"),
    ("clankerdiff-client", "crates/diff/client"),
    ("clankerdiff-git", "crates/diff/git"),
    ("clankerdiff-watch", "crates/diff/watch"),
    ("clankerdiff-server", "crates/diff/server"),
    ("clankerdiff-markdown", "crates/diff/markdown"),
    ("clankerdiff-symbols", "crates/diff/symbols"),
    ("clankerdiff-rouge", "crates/diff/rouge"),
    ("clankerdiff-ratatui", "crates/diff/ratatui"),
    ("clankerdiff-desktop", "crates/diff/desktop"),
    ("clankerdiff-web", "crates/diff/web"),
];

pub struct RougeApp {
    diff: String,
    crates: Vec<String>,
}

impl RougeApp {
    #[must_use]
    pub fn new(diff: &str) -> Self {
        Self { diff: diff.into(), crates: Vec::new() }
    }

    #[must_use]
    pub fn crates<T: Into<String>>(mut self, names: impl IntoIterator<Item = T>) -> Self {
        self.crates.extend(names.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn linux_dependencies() -> Step {
        Step::run(format!(
            "sudo apt-get update && sudo apt-get install --yes {} mesa-vulkan-drivers",
            ROUGE_LINUX_PACKAGES.join(" "),
        ))
        .name("Install Linux build dependencies")
    }

    fn crate_path(&self, name: &str) -> String {
        let (_, relative) = CRATES
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .unwrap_or_else(|| panic!("`{name}` is not a crate in the diff repository"));
        if self.diff == "." { (*relative).to_owned() } else { format!("{}/{relative}", self.diff) }
    }
}

impl Preset for RougeApp {
    fn apply(self, project: &mut Project) {
        let mut keys = TomlKeys::new("Cargo.toml");
        for name in ROUGE_CRATES {
            keys = keys.set(KeyPath::new(["workspace", "dependencies", name]), "0.2.0");
        }
        for name in &self.crates {
            keys = keys.set(KeyPath::new(["workspace", "dependencies", name, "path"]), self.crate_path(name));
        }
        project.add(keys);
    }
}
