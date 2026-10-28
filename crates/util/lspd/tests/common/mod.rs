pub(crate) use clankerdiff_lspd::testing::TestWorkspace;

use clankerdiff_lspd::testing::{TestDaemon, TestDaemonBuilder};
use clankerdiff_lspd::{ClientError, LanguageId, LspClient, path_to_uri};
use lsp_types::{Hover, HoverContents, PublishDiagnosticsParams, Uri};
use serde_json::{Value, json};
use std::error::Error;
use std::path::Path;
use std::time::Duration;
use tokio::time::{sleep, timeout};

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn Error>>;

pub(crate) fn fake(root: &Path, language: LanguageId, args: &[&str]) -> TestDaemonBuilder {
    TestDaemon::builder(root, language).fake_server(args)
}

pub(crate) async fn serve(builder: TestDaemonBuilder) -> TestResult<(TestDaemon, LspClient)> {
    let daemon = builder.spawn()?;
    let client = daemon.client().await?;
    Ok((daemon, client))
}

pub(crate) async fn fake_request(client: &LspClient, method: &str) -> Result<(), ClientError> {
    client.call(method, &json!({}), || Value::Null).await.map(|_: Value| ())
}

pub(crate) async fn wait_for_workspace_diagnostics(
    client: &LspClient,
    predicate: impl Fn(&[PublishDiagnosticsParams]) -> bool,
) -> TestResult<Vec<PublishDiagnosticsParams>> {
    let poll = async {
        loop {
            let diagnostics = client.get_diagnostics(None).await?;
            if predicate(&diagnostics) {
                return Ok::<_, Box<dyn Error>>(diagnostics);
            }
            sleep(Duration::from_millis(50)).await;
        }
    };
    timeout(Duration::from_secs(30), poll).await?
}

pub(crate) fn workspace_error_count(diagnostics: &[PublishDiagnosticsParams]) -> usize {
    diagnostics.iter().map(|params| params.diagnostics.len()).sum()
}

pub(crate) fn hover_text(hover: Option<Hover>) -> TestResult<String> {
    match hover {
        Some(Hover { contents: HoverContents::Markup(markup), .. }) => Ok(markup.value),
        other => Err(format!("expected a markup hover, got {other:?}").into()),
    }
}

pub(crate) fn cargo_project(name: &str) -> TestWorkspace {
    TestWorkspace::new()
        .file("Cargo.toml", format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"))
        .file("src/main.rs", "fn main() {}\n")
}

pub(crate) fn file_uri(path: &Path) -> Uri {
    path_to_uri(path).expect("Invalid file path")
}
