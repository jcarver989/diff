use diff_framer::{CRATES, GpuiApp};
use framer::Project;
use framer::github::Step;
use framer::rust::{Level, Lints};
use framer::toml_edit::{Array, InlineTable, Value};
use framer_presets::{Ci, CompilingJob, Dist, Release, ReleasePlz, Rust, RustMonorepo, Rustfmt, Tools, Workspace};
use std::process::ExitCode;

fn main() -> ExitCode {
    let monorepo = RustMonorepo::new()
        .rust(Rust::new("1.98").toolchain("1.99.1").targets(["wasm32-unknown-unknown"]))
        .workspace(workspace())
        .lints(lints())
        .rustfmt(rustfmt())
        .tools(Tools::new().tool("actionlint", "1.7.12"))
        .ci(ci())
        .without_conventions()
        .release(release());
    let gpui = GpuiApp::new(".").crates(CRATES.iter().map(|(name, _)| *name)).web();

    let mut project = Project::new();
    project.apply(monorepo);
    project.apply(gpui);
    project.toml("clippy.toml").set("disallowed-methods", disallowed_radii());
    project.synth()
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
    let reason = "use a radius token from clankerdiff_gpui_ui::tokens, or rounded_full() for a capsule";
    let mut paths = ["", "_t", "_b", "_l", "_r", "_tl", "_tr", "_bl", "_br"]
        .into_iter()
        .flat_map(|side| {
            ["xs", "sm", "md", "lg", "xl", "2xl", "3xl"].map(move |size| format!("gpui::Styled::rounded{side}_{size}"))
        })
        .map(|path| {
            let mut entry = InlineTable::new();
            entry.insert("path", path.into());
            entry.insert("reason", reason.into());
            let mut entry = Value::InlineTable(entry);
            entry.decor_mut().set_prefix("\n    ");
            entry
        })
        .collect::<Array>();
    paths.set_trailing("\n");
    paths.set_trailing_comma(true);
    paths
}

fn rustfmt() -> Rustfmt {
    Rustfmt::new().set("max_width", 120).set("newline_style", "Unix").set("use_small_heuristics", "Max")
}

fn ci() -> Ci {
    Ci::new().setup(GpuiApp::linux_dependencies()).compiling_job(features_job()).compiling_job(wasm_job())
}

fn features_job() -> CompilingJob {
    CompilingJob::new("features", "Minimal features", "feature-check")
        .step(Step::run("just remote-test").name("Test remote backend and headless CLI"))
}

fn wasm_job() -> CompilingJob {
    let trunk = Step::uses("jetli/trunk-action@v0.5.1").with("version", "v0.21.14");
    let browser_deps = Step::run("cd crates/diff/web && npm ci && npx playwright install --with-deps chromium")
        .name("Install browser test dependencies");
    let smoke_test = Step::run("cd crates/diff/web && npm test").name("Run browser highlighting smoke test");
    CompilingJob::new("wasm", "WASM", "wasm-check").before(trunk).step(browser_deps).step(smoke_test)
}

fn release() -> Release {
    let release_plz = ReleasePlz::new()
        .publish_to_crates_io()
        .setup(GpuiApp::linux_dependencies())
        .set("release", false)
        .set("git_only", false)
        .set("publish", false);
    Release::new().release_plz(release_plz).dist(Dist::new().allow_dirty_ci())
}
