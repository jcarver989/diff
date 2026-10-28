use crate::common::{TestResult, cargo_project, fake, file_uri, hover_text, serve};
use clankerdiff_lspd::testing::{TestGate, TestPoint};
use clankerdiff_lspd::{ClientError, LanguageId};
use std::pin::pin;
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::test]
async fn wedged_and_crashed_servers_are_replaced_on_the_next_request() -> TestResult {
    let project = cargo_project("recovery");
    let args = ["--wedge-on", "textDocument/definition", "--crash-on", "textDocument/references"];
    let received = TestGate::default();
    let builder = fake(project.root(), LanguageId::Rust, &args)
        .paused_time()
        .request_timeout(REQUEST_TIMEOUT)
        .gate(TestPoint::RequestReceived("textDocument/definition".to_owned()), &received);
    let (daemon, client) = serve(builder).await?;
    let uri = file_uri(&project.path("src/main.rs"));

    let mut wedged = pin!(client.definition_locations(uri.clone(), 0, 3));
    received.reached_before(wedged.as_mut()).await;
    received.release();
    daemon.advance_time(REQUEST_TIMEOUT).await;
    let wedged = wedged.await;
    assert!(matches!(wedged, Err(ClientError::TimedOut { .. })), "{wedged:?}");
    assert!(hover_text(client.hover(uri.clone(), 0, 0).await?)?.contains("open_count=1"));

    let crashed = client.find_references(uri.clone(), 0, 0, true).await;
    assert!(crashed.is_err(), "request to a crashing server should fail, got {crashed:?}");
    assert!(hover_text(client.hover(uri, 0, 0).await?)?.contains("open_count=1"));
    Ok(())
}

#[tokio::test]
async fn an_invalid_server_message_does_not_break_the_session() -> TestResult {
    let project = cargo_project("garbled_message");
    let (_daemon, client) =
        serve(fake(project.root(), LanguageId::Rust, &["--garble-before", "textDocument/hover"])).await?;

    let hover = client.hover(file_uri(&project.path("src/main.rs")), 0, 0).await?;

    assert!(hover.is_some(), "the response after an invalid server message should still arrive");
    Ok(())
}
