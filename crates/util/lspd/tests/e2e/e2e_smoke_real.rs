use crate::common::{TestResult, cargo_project, file_uri, hover_text, serve};
use clankerdiff_lspd::LanguageId;
use clankerdiff_lspd::testing::TestDaemon;

#[tokio::test]
#[ignore = "requires rust-analyzer on PATH"]
async fn rust_analyzer_smoke_hover() -> TestResult {
    let project = cargo_project("smoke_ra").file("src/main.rs", "fn main() { let value = 1; }\n");
    let (_daemon, client) = serve(TestDaemon::builder(project.root(), LanguageId::Rust)).await?;

    let text = hover_text(client.hover(file_uri(&project.path("src/main.rs")), 0, 16).await?)?;

    assert!(text.contains("i32"), "expected the inferred type in hover: {text}");
    Ok(())
}
