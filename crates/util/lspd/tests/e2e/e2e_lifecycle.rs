use crate::common::{
    TestResult, TestWorkspace, cargo_project, fake, file_uri, hover_text, serve, wait_for_workspace_diagnostics,
    workspace_error_count,
};
use clankerdiff_lspd::LanguageId;
use clankerdiff_lspd::testing::{TestGate, TestPoint};
use std::time::Duration;
use tokio::time::{sleep, timeout};

#[tokio::test]
async fn the_daemon_keeps_serving_after_clients_disconnect_or_drop() -> TestResult {
    let project = cargo_project("client_departures");
    let (daemon, client) = serve(fake(project.root(), LanguageId::Rust, &[])).await?;

    drop(client);
    drop(daemon.client().await?);

    let hover = daemon.client().await?.hover(file_uri(&project.path("src/main.rs")), 0, 0).await?;
    assert!(hover.is_some());
    Ok(())
}

#[tokio::test]
async fn a_client_that_disconnects_mid_request_releases_the_document_it_opened() -> TestResult {
    let project = TestWorkspace::new().file("index.ts", "const ok = 1;\n");
    let args = ["--pull-diagnostics", "--wedge-on", "textDocument/definition"];
    let received = TestGate::default();
    let builder = fake(project.root(), LanguageId::TypeScript, &args)
        .gate(TestPoint::RequestReceived("textDocument/definition".to_owned()), &received);
    let (daemon, client) = serve(builder).await?;
    assert_eq!(workspace_error_count(&client.get_diagnostics(None).await?), 0);

    let abandoned = daemon.client().await?;
    let uri = file_uri(&project.path("index.ts"));
    let request = tokio::spawn(async move { abandoned.definition_locations(uri, 0, 6).await });
    received.reached().await;
    received.release();
    request.abort();
    assert!(request.await.is_err_and(|error| error.is_cancelled()));

    project.write("index.ts", "const error = 1;\n");
    let refreshed =
        wait_for_workspace_diagnostics(&client, |diagnostics| workspace_error_count(diagnostics) > 0).await?;
    assert_eq!(workspace_error_count(&refreshed), 1);
    Ok(())
}

#[tokio::test]
async fn a_request_its_caller_stops_waiting_for_is_cancelled_on_the_language_server() -> TestResult {
    let project = cargo_project("abandoned_request");
    let (received, cancelled) = (TestGate::default(), TestGate::default());
    let builder = fake(project.root(), LanguageId::Rust, &["--wedge-on", "textDocument/hover"])
        .gate(TestPoint::RequestReceived("textDocument/hover".to_owned()), &received)
        .gate(TestPoint::RequestReceived("$/cancelRequest".to_owned()), &cancelled);
    let (_daemon, client) = serve(builder).await?;
    let uri = file_uri(&project.path("src/main.rs"));

    let waiting = client.clone();
    let request = tokio::spawn(async move { waiting.hover(uri, 0, 0).await });
    received.reached().await;
    received.release();
    request.abort();

    timeout(Duration::from_secs(10), cancelled.reached()).await?;
    cancelled.release();
    assert!(client.is_connected());
    Ok(())
}

#[tokio::test]
async fn clients_reconnect_when_the_daemon_restarts() -> TestResult {
    let project = cargo_project("reconnect");
    let (daemon, client) = serve(fake(project.root(), LanguageId::Rust, &[])).await?;
    assert!(client.is_connected());

    daemon.shutdown()?;
    timeout(Duration::from_secs(10), async {
        while client.is_connected() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let _daemon = fake(project.root(), LanguageId::Rust, &[]).spawn()?;

    assert!(client.hover(file_uri(&project.path("src/main.rs")), 0, 0).await?.is_some());
    assert!(client.is_connected());
    Ok(())
}

#[tokio::test]
async fn clients_share_one_server_session_and_its_diagnostics() -> TestResult {
    let project = cargo_project("shared_session").file("src/main.rs", "fn main() { let error = 1; }\n");
    let (daemon, first) = serve(fake(project.root(), LanguageId::Rust, &[])).await?;
    let second = daemon.client().await?;
    let uri = file_uri(&project.path("src/main.rs"));

    let hover = hover_text(first.hover(uri.clone(), 0, 0).await?)?;
    assert_ne!(hover, "");
    assert_eq!(hover, hover_text(second.hover(uri.clone(), 0, 0).await?)?);

    for client in [&first, &second] {
        let diagnostics = client.get_diagnostics(Some(uri.clone())).await?;
        assert_eq!((diagnostics.len(), workspace_error_count(&diagnostics)), (1, 1));
    }
    Ok(())
}

#[tokio::test]
async fn workspace_diagnostics_refresh_after_external_edit_without_explicit_open() -> TestResult {
    let project = cargo_project("workspace_external_refresh").file("src/main.rs", "fn main() { let ok = 1; }\n");
    let (_daemon, client) = serve(fake(project.root(), LanguageId::Rust, &[])).await?;

    let initial = wait_for_workspace_diagnostics(&client, |diagnostics| diagnostics.len() == 1).await?;
    assert_eq!(workspace_error_count(&initial), 0);

    project.write("src/main.rs", "fn main() { let error = 1; }\n");

    let refreshed =
        wait_for_workspace_diagnostics(&client, |diagnostics| workspace_error_count(diagnostics) > 0).await?;
    assert_eq!((refreshed.len(), workspace_error_count(&refreshed)), (1, 1));
    Ok(())
}

#[tokio::test]
async fn only_a_queued_refresh_after_an_edit_saves_the_document() -> TestResult {
    let project = cargo_project("saves").file("src/lib.rs", "pub fn lib() {}\n");
    let (_daemon, client) = serve(fake(project.root(), LanguageId::Rust, &[])).await?;
    let uri = file_uri(&project.path("src/main.rs"));
    client.get_diagnostics(None).await?;

    let hover = hover_text(client.hover(uri.clone(), 0, 0).await?)?;
    assert!(hover.contains("save_count=0"), "{hover}");

    project.write("src/main.rs", "fn main() { let edited = 1; }\n");
    client.queue_diagnostic_refresh(uri.clone()).await?;
    client.get_diagnostics(None).await?;

    let hover = hover_text(client.hover(uri, 0, 0).await?)?;
    assert!(hover.contains("save_count=1"), "{hover}");
    Ok(())
}
