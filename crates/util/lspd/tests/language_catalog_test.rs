use clankerdiff_lspd::testing::TestWorkspace;
use clankerdiff_lspd::{LanguageId, detect_project_languages};
use std::path::Path;

#[test]
fn languages_are_detected_from_paths() {
    assert_eq!(LanguageId::from_path(Path::new("app.tsx")), Some(LanguageId::TypeScriptReact));
    assert_eq!(LanguageId::from_path(Path::new("bar.py")), Some(LanguageId::Python));
    assert_eq!(LanguageId::from_path(Path::new("notes.md")), None);
    assert_eq!(LanguageId::from_path(Path::new("no_extension")), None);
    assert_eq!(LanguageId::TypeScriptReact.as_str(), "typescriptreact");
}

#[test]
fn project_languages_are_detected_from_manifests() {
    let project = TestWorkspace::new().file("Cargo.toml", "").file("package.json", "{}");
    assert_eq!(detect_project_languages(project.root()), [LanguageId::Rust, LanguageId::TypeScript]);
}
