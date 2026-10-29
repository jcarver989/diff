use framer::github::Step;
use framer::toml_edit::{Array, InlineTable, Value};
use framer::{KeyPath, Preset, Project, TomlKeys};

const GPUI_VERSION: &str = "=0.3.7";
const GPUI_BASE_VERSION: &str = "=0.7.0";
const GPUI_PLATFORM_FEATURES: [&str; 4] = ["font-kit", "runtime_shaders", "wayland", "x11"];

pub const CRATES: [(&str, &str); 18] = [
    ("clankerdiff-fingerprint", "crates/util/fingerprint"),
    ("clankerdiff-remote", "crates/util/remote"),
    ("clankerdiff-lspd", "crates/util/lspd"),
    ("clankerdiff-ui-core", "crates/ui/core"),
    ("clankerdiff-syntax", "crates/ui/syntax"),
    ("clankerdiff-gpui-ui", "crates/ui/gpui"),
    ("clankerdiff-gpui-shell", "crates/ui/gpui-shell"),
    ("clankerdiff-core", "crates/diff/core"),
    ("clankerdiff-protocol", "crates/diff/protocol"),
    ("clankerdiff-client", "crates/diff/client"),
    ("clankerdiff-git", "crates/diff/git"),
    ("clankerdiff-watch", "crates/diff/watch"),
    ("clankerdiff-server", "crates/diff/server"),
    ("clankerdiff-markdown", "crates/diff/markdown"),
    ("clankerdiff-symbols", "crates/diff/symbols"),
    ("clankerdiff-gpui", "crates/diff/gpui"),
    ("clankerdiff-ratatui", "crates/diff/ratatui"),
    ("clankerdiff-gpui-desktop", "crates/diff/desktop"),
];

pub struct GpuiApp {
    diff: String,
    crates: Vec<String>,
    web: bool,
}

impl GpuiApp {
    #[must_use]
    pub fn new(diff: &str) -> Self {
        Self { diff: diff.into(), crates: Vec::new(), web: false }
    }

    #[must_use]
    pub fn crates<T: Into<String>>(mut self, names: impl IntoIterator<Item = T>) -> Self {
        self.crates.extend(names.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn web(mut self) -> Self {
        self.web = true;
        self
    }

    #[must_use]
    pub fn linux_dependencies() -> Step {
        Step::run("sudo apt-get update && sudo apt-get install --yes libfontconfig1-dev libxkbcommon-x11-dev")
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

impl Preset for GpuiApp {
    fn apply(self, project: &mut Project) {
        let mut keys = TomlKeys::new("Cargo.toml")
            .set(dependency("gpui"), gpui("gpui-pre").with("default-features", false))
            .set(dependency("gpui_platform"), gpui("gpui-pre-platform").with("features", features()))
            .set(dependency("gpui-base"), GPUI_BASE_VERSION);
        if self.web {
            keys = keys.set(dependency("gpui_web"), gpui("gpui-pre-web").with("default-features", false));
        }
        for name in &self.crates {
            keys = keys.set(KeyPath::new(["workspace", "dependencies", name, "path"]), self.crate_path(name));
        }
        project.add(keys);
    }
}

struct Table(InlineTable);

impl Table {
    fn with(mut self, key: &str, setting: impl Into<Value>) -> Self {
        self.0.insert(key, setting.into());
        self
    }
}

impl From<Table> for Value {
    fn from(table: Table) -> Self {
        Value::InlineTable(table.0)
    }
}

fn dependency(name: &str) -> KeyPath {
    KeyPath::new(["workspace", "dependencies", name])
}

fn gpui(package: &str) -> Table {
    Table(InlineTable::new()).with("package", package).with("version", GPUI_VERSION)
}

fn features() -> Array {
    GPUI_PLATFORM_FEATURES.iter().map(|feature| Value::from(*feature)).collect()
}
