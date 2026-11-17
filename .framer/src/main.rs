use diff_framer::{CRATES, ROUGE_LINUX_PACKAGES, RougeApp};
use framer::github::Step;
use framer::just::Justfile;
use framer::rust::{Level, Lints};
use framer::toml_edit::{Array, ArrayOfTables, InlineTable, Item, Table, Value};
use framer::{KeyPath, Project, TomlKeys};
use framer_presets::{Ci, CompilingJob, Dist, Release, ReleasePlz, Rust, RustMonorepo, Rustfmt, Tools, Workspace};
use std::process::ExitCode;

fn main() -> ExitCode {
    let monorepo = RustMonorepo::new()
        .rust(Rust::new("1.100").toolchain("1.100.0").targets(["wasm32-unknown-unknown"]))
        .workspace(workspace())
        .lints(lints())
        .rustfmt(rustfmt())
        .tools(Tools::new().tool("actionlint", "1.7.12"))
        .ci(ci())
        .without_conventions()
        .release(release());
    let rouge = RougeApp::new(".").crates(
        CRATES.iter().map(|(name, _)| *name).filter(|name| !matches!(*name, "clankerdiff-remote" | "clankerdiff-web")),
    );

    let mut project = Project::new();
    project.apply(monorepo);
    project.apply(rouge);
    project.add(TomlKeys::new("release-plz.toml").set_item("package", Item::ArrayOfTables(publishable_packages())));
    configure_cargo(&mut project);
    configure_dist_recipes(&mut project);
    project.toml("clippy.toml").set("disallowed-methods", disallowed_radii());
    project.synth()
}

fn configure_cargo(project: &mut Project) {
    project.add(TomlKeys::new(".cargo/config.toml").set("registry.global-min-publish-age", "7 days"));
    project
        .toml("Cargo.toml")
        .set("workspace.lints.cargo.unused_dependencies", "deny")
        .set("workspace.lints.cargo.unused_workspace_dependencies", "deny");
    let mut dependencies = TomlKeys::new("Cargo.toml");
    for name in ["clankerdiff-remote", "clankerdiff-web", "open", "pulldown-cmark", "schemars", "tree-sitter"] {
        dependencies = dependencies.set_item(KeyPath::new(["workspace", "dependencies", name]), Item::None);
    }
    project.add(dependencies);
}

fn workspace() -> Workspace {
    Workspace::new()
        .members(["crates/util/*", "crates/ui/*", "crates/diff/*"])
        .extra_manifests(["tests/consumer-fixture/Cargo.toml"])
        .repository("https://github.com/jcarver989/diff")
}

fn lints() -> Lints {
    Lints::clippy_pedantic()
        .allow_clippy(["missing_errors_doc", "missing_panics_doc"])
        .rust("unsafe_code", Level::Forbid)
}

fn disallowed_radii() -> Array {
    let mut entry = InlineTable::new();
    entry.insert("path", "rouge_render::Corners::all".into());
    entry.insert("reason", "use a shared radius token from rouge_widgets::tokens".into());
    entry.insert("allow-invalid", true.into());
    [Value::InlineTable(entry)].into_iter().collect()
}

fn configure_dist_recipes(project: &mut Project) {
    let recipes = project.get_mut::<Justfile>().expect("the monorepo preset provides recipes");
    for (name, command) in [
        ("dist-generate", "python3 scripts/generate-dist-workflow.py"),
        ("dist-check", "python3 scripts/generate-dist-workflow.py --check"),
    ] {
        *recipes.recipe_mut(name).expect("the dist preset provides the recipe").lines_mut() = vec![command.into()];
    }
}

fn publishable_packages() -> ArrayOfTables {
    CRATES
        .iter()
        .filter(|(_, path)| !path.contains("/rouge") && !path.ends_with("/desktop") && !path.ends_with("/web"))
        .map(|(name, _)| {
            let mut package = Table::new();
            package["name"] = framer::toml_edit::value(*name);
            package["release"] = framer::toml_edit::value(true);
            package["publish"] = framer::toml_edit::value(true);
            package
        })
        .collect()
}

fn rustfmt() -> Rustfmt {
    Rustfmt::new().set("max_width", 120).set("newline_style", "Unix").set("use_small_heuristics", "Max")
}

fn ci() -> Ci {
    Ci::new().setup(RougeApp::linux_dependencies()).compiling_job(features_job()).compiling_job(wasm_job())
}

fn features_job() -> CompilingJob {
    CompilingJob::new("features", "Minimal features", "feature-check")
        .step(Step::run("just remote-test").name("Test remote backend and headless CLI"))
}

fn wasm_job() -> CompilingJob {
    let trunk = Step::uses("jetli/trunk-action@v0.5.1").with("version", "v0.21.14");
    let browser_deps = Step::run("cd crates/diff/web && npm ci && npx playwright install --with-deps chromium")
        .name("Install browser test dependencies");
    let smoke_test = Step::run("cd crates/diff/web && npm test").name("Run Rouge browser contract tests");
    CompilingJob::new("wasm", "WASM", "wasm-check").before(trunk).step(browser_deps).step(smoke_test)
}

fn release() -> Release {
    let release_plz = ReleasePlz::new()
        .publish_to_crates_io()
        .setup(RougeApp::linux_dependencies())
        .set("release", false)
        .set("git_only", false)
        .set("publish", false)
        .set("semver_check", false);
    let apt: InlineTable = ROUGE_LINUX_PACKAGES.into_iter().map(|package| (package, Value::from("*"))).collect();
    Release::new()
        .release_plz(release_plz)
        .dist(Dist::new().allow_dirty_ci().set("system-dependencies.apt", Value::InlineTable(apt)))
}
