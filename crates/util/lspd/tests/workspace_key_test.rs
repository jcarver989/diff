use clankerdiff_lspd::testing::TestWorkspace;
use clankerdiff_lspd::{LanguageId, WorkspaceKey};
use std::error::Error;

const PACKAGE: &str = "[package]\nname = \"member\"\nversion = \"0.1.0\"\n";

#[test]
fn workspace_roots_follow_each_languages_project_conventions() -> Result<(), Box<dyn Error>> {
    let cases = [
        (
            "the outermost cargo workspace",
            TestWorkspace::new()
                .file("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n")
                .file("crates/member/Cargo.toml", PACKAGE)
                .file("crates/member/src/lib.rs", ""),
            "crates/member/src",
            LanguageId::Rust,
            "",
        ),
        (
            "the nearest crate outside a workspace",
            TestWorkspace::new()
                .file("Cargo.toml", PACKAGE)
                .file("tools/nested/Cargo.toml", PACKAGE)
                .file("tools/nested/src/main.rs", ""),
            "tools/nested/src/main.rs",
            LanguageId::Rust,
            "tools/nested",
        ),
        (
            "a go.work over its module",
            TestWorkspace::new()
                .file("go.work", "go 1.23\n")
                .file("service/go.mod", "module service\n")
                .file("service/api/.keep", ""),
            "service/api",
            LanguageId::Go,
            "",
        ),
        (
            "a package with workspaces",
            TestWorkspace::new()
                .file("package.json", "{ \"workspaces\": [\"packages/*\"] }")
                .file("packages/app/package.json", "{ \"name\": \"app\" }")
                .file("packages/app/tsconfig.json", "{}")
                .file("packages/app/src/.keep", ""),
            "packages/app/src",
            LanguageId::TypeScript,
            "",
        ),
        (
            "the nearest typescript project without workspaces",
            TestWorkspace::new()
                .file("package.json", "{ \"name\": \"site\" }")
                .file("packages/app/tsconfig.json", "{}")
                .file("packages/app/src/.keep", ""),
            "packages/app/src",
            LanguageId::TypeScriptReact,
            "packages/app",
        ),
        (
            "the nearest python project file",
            TestWorkspace::new().file("pyproject.toml", "").file("tool/setup.py", "").file("tool/pkg/.keep", ""),
            "tool/pkg",
            LanguageId::Python,
            "tool",
        ),
        (
            "the repository root without project files",
            TestWorkspace::new().file(".git/.keep", "").file("src/deep/.keep", ""),
            "src/deep",
            LanguageId::Go,
            "",
        ),
        (
            "the repository, ignoring project files above it",
            TestWorkspace::new()
                .file("Cargo.toml", "[workspace]\n")
                .file("checkout/.git/.keep", "")
                .file("checkout/Cargo.toml", PACKAGE),
            "checkout",
            LanguageId::Rust,
            "checkout",
        ),
        (
            "the path itself without a project or repository",
            TestWorkspace::new().file("notes/.keep", ""),
            "notes",
            LanguageId::C,
            "notes",
        ),
    ];
    for (expected_root, project, start, language, root) in cases {
        let expected = project.path(root).canonicalize()?;
        assert_eq!(WorkspaceKey::resolve(&project.path(start), language).root(), expected, "expected {expected_root}");
    }
    Ok(())
}

#[test]
fn languages_served_by_one_server_share_a_workspace() {
    let (project, other) = (TestWorkspace::new(), TestWorkspace::new());
    let key = |language| WorkspaceKey::resolve(project.root(), language);
    assert_eq!(key(LanguageId::TypeScript), key(LanguageId::TypeScriptReact));
    assert_eq!(key(LanguageId::C), key(LanguageId::Cpp));
    assert_ne!(key(LanguageId::Rust), key(LanguageId::Python));
    assert_ne!(key(LanguageId::Rust), WorkspaceKey::resolve(other.root(), LanguageId::Rust));
}
