use framer::github::Step;
use framer::toml_edit::{InlineTable, Value};
use framer::{KeyPath, Preset, Project, TomlKeys};

const ROUGE_REPOSITORY: &str = "https://github.com/jcarver989/rouge.git";
const ROUGE_REVISION: &str = "df25bbf3375f2609cbc33cdff445b633fbc300db";
pub const ROUGE_LINUX_PACKAGES: [&str; 3] = ["libfontconfig1-dev", "libxkbcommon-dev", "libwayland-dev"];
const ROUGE_CRATES: [&str; 7] =
    ["rouge", "rouge-core", "rouge-reactive", "rouge-widgets", "rouge-theme", "rouge-syntax", "rouge-markdown"];

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
    rouge: String,
    crates: Vec<String>,
    web: bool,
}

impl RougeApp {
    #[must_use]
    pub fn new(diff: &str) -> Self {
        Self { diff: diff.into(), rouge: "../rouge".into(), crates: Vec::new(), web: false }
    }

    #[must_use]
    pub fn rouge(mut self, path: &str) -> Self {
        self.rouge = path.into();
        self
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
        Step::run(format!(
            "sudo apt-get update && sudo apt-get install --yes {} mesa-vulkan-drivers",
            ROUGE_LINUX_PACKAGES.join(" "),
        ))
        .name("Install Linux build dependencies")
    }

    #[must_use]
    pub fn checkout_step() -> Step {
        Step::run("bash .github/scripts/checkout-rouge.sh").name("Check out pinned Rouge sibling")
    }

    #[must_use]
    pub fn checkout_script() -> String {
        format!(
            r#"set -euo pipefail

revision={ROUGE_REVISION}
if [[ -e ../rouge ]]; then
    if [[ $(git -C ../rouge rev-parse --verify HEAD 2>/dev/null) == "$revision" && -f ../rouge/crates/rouge-web/Cargo.toml ]]; then
        exit 0
    fi
    echo 'Expected the pinned Rouge checkout in ../rouge; refusing to replace the existing path.' >&2
    exit 1
fi

checkout=$(mktemp -d ../rouge.XXXXXX)
trap 'rm -rf "$checkout"' EXIT
git -C "$checkout" init --quiet
git -C "$checkout" remote add origin {ROUGE_REPOSITORY}
git -C "$checkout" fetch --depth=1 origin "$revision"
git -C "$checkout" checkout --quiet --detach FETCH_HEAD
mv "$checkout" ../rouge
"#,
        )
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
        for name in ROUGE_CRATES.into_iter().chain(self.web.then_some("rouge-web")) {
            let mut dependency = InlineTable::new();
            dependency.insert("path", format!("{}/crates/{name}", self.rouge).into());
            keys = keys.set(KeyPath::new(["workspace", "dependencies", name]), Value::InlineTable(dependency));
        }
        for name in &self.crates {
            keys = keys.set(KeyPath::new(["workspace", "dependencies", name, "path"]), self.crate_path(name));
        }
        project.add(keys);
    }
}
