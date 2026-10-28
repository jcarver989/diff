use crate::common::{
    TestResult, cargo_project, fake, fake_request, file_uri, hover_text, serve, workspace_error_count,
};
use clankerdiff_lspd::testing::{TestGate, TestPoint};
use clankerdiff_lspd::{LanguageId, uri_to_path};
use futures::future::join_all;
use lsp_types::DocumentSymbolResponse;
use std::pin::pin;

#[tokio::test]
async fn request_helpers_round_trip_through_fake_server() -> TestResult {
    let content = "fn example_fn() -> i32 {\n    7\n}\n\nfn main() {\n    let value = example_fn();\n    println!(\"{}\", value);\n}\n";
    let project = cargo_project("request_contracts").file("src/main.rs", content);
    let (_daemon, client) = serve(fake(project.root(), LanguageId::Rust, &[])).await?;
    let uri = file_uri(&project.path("src/main.rs"));

    let hover = hover_text(client.hover(uri.clone(), 5, 16).await?)?;
    assert!(hover.contains("open_count=1; line=5; character=16") && hover.contains("example_fn"), "{hover}");

    let definitions = client.definition_locations(uri.clone(), 5, 16).await?;
    assert!(definitions.iter().all(|location| location.uri == uri));
    assert_eq!(definitions.iter().map(|location| location.range.start.line).collect::<Vec<_>>(), [0]);
    let references = client.find_references(uri.clone(), 0, 3, true).await?;
    assert_eq!(references.iter().map(|location| location.range.start.line).collect::<Vec<_>>(), [0, 5]);

    let DocumentSymbolResponse::Flat(symbols) = client.document_symbol(uri.clone()).await? else {
        return Err("expected flat document symbols".into());
    };
    assert_eq!(symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(), ["ExampleStruct", "example_fn"]);

    let rename = client.rename(uri.clone(), 0, 3, "renamed_fn".to_owned()).await?.ok_or("a workspace edit")?;
    #[allow(clippy::mutable_key_type)]
    let changes = rename.changes.ok_or("rename changes")?;
    let edits = changes.get(&uri).ok_or("edits for the renamed file")?;
    assert_eq!(edits.iter().map(|edit| edit.new_text.as_str()).collect::<Vec<_>>(), ["renamed_fn", "renamed_fn"]);
    Ok(())
}

#[tokio::test]
async fn diagnostics_round_trip_for_documents_in_paths_with_spaces() -> TestResult {
    let project = cargo_project("request_spaces").file("src/my mod.rs", "fn main() { let error = 1; }\n");
    let (_daemon, client) = serve(fake(project.root(), LanguageId::Rust, &[])).await?;
    let uri = file_uri(&project.path("src/my mod.rs"));
    assert!(uri.as_str().contains("my%20mod.rs"), "URI must be percent-encoded: {}", uri.as_str());
    assert_eq!(uri_to_path(&uri), project.path("src/my mod.rs").canonicalize()?);

    client.queue_diagnostic_refresh(uri.clone()).await?;

    let diagnostics = client.get_diagnostics(None).await?;
    let ours = diagnostics.iter().find(|entry| entry.uri == uri).ok_or("diagnostics for the file")?;
    let messages: Vec<_> = ours.diagnostics.iter().map(|diagnostic| diagnostic.message.as_str()).collect();
    assert_eq!(messages, ["error token"]);
    Ok(())
}

#[tokio::test]
async fn a_pending_diagnostics_call_does_not_block_other_requests_on_the_connection() -> TestResult {
    let project =
        cargo_project("held_diagnostics").file("src/main.rs", "// hold-diagnostics\nfn main() { let error = 1; }\n");
    let (_daemon, client) = serve(fake(project.root(), LanguageId::Rust, &[])).await?;
    let uri = file_uri(&project.path("src/main.rs"));

    let (diagnostics, hover) = tokio::join!(client.get_diagnostics(Some(uri.clone())), async {
        let hover = client.hover(uri.clone(), 0, 0).await;
        fake_request(&client, "fake/releaseDiagnostics").await.map(|()| hover)
    });

    assert!(hover??.is_some());
    assert_eq!(workspace_error_count(&diagnostics?), 1);
    Ok(())
}

#[tokio::test]
async fn position_requests_wait_for_the_server_to_finish_loading() -> TestResult {
    let project = cargo_project("loading_server").file(".fake-loading", "");
    let waiting = TestGate::default();
    let (_daemon, client) =
        serve(fake(project.root(), LanguageId::Rust, &[]).gate(TestPoint::ReadyWait, &waiting)).await?;
    let mut hover = pin!(client.hover(file_uri(&project.path("src/main.rs")), 0, 0));

    waiting.reached_before(hover.as_mut()).await;
    fake_request(&client, "fake/finishLoading").await?;

    assert!(hover_text(hover.await?)?.contains("open_count=1"));
    Ok(())
}

#[tokio::test]
async fn concurrent_requests_for_one_document_reach_the_server_after_it_opens() -> TestResult {
    let project = cargo_project("concurrent_open").file("target/pending.rs", "fn pending() {}\n");
    let uri = file_uri(&project.path("target/pending.rs"));
    let (opening, waiting) = (TestGate::default(), TestGate::default());
    let builder = fake(project.root(), LanguageId::Rust, &[])
        .gate(TestPoint::DocumentOpen(uri.clone()), &opening)
        .gate(TestPoint::DocumentSyncWait(uri.clone()), &waiting);
    let (_daemon, client) = serve(builder).await?;

    let mut first = pin!(client.hover(uri.clone(), 0, 0));
    opening.reached_before(first.as_mut()).await;
    let mut second = pin!(client.hover(uri, 0, 0));
    waiting.reached_before(second.as_mut()).await;
    opening.release();

    let (first, second) = tokio::join!(first, second);
    for hover in [first?, second?] {
        assert!(hover_text(hover)?.contains("open_count=1"));
    }
    Ok(())
}

#[tokio::test]
async fn diagnostics_waiters_observe_concurrent_publications() -> TestResult {
    let project = cargo_project("concurrent_diagnostics");
    let targets: Vec<_> = (0..64)
        .map(|index| {
            let path =
                project.write(format!("target/file_{index}.rs"), "// hold-diagnostics\nfn f() { let error = 1; }\n");
            (file_uri(&path), TestGate::default())
        })
        .collect();
    let builder = targets.iter().fold(fake(project.root(), LanguageId::Rust, &[]), |builder, (uri, gate)| {
        builder.gate(TestPoint::DiagnosticsWait(uri.clone()), gate)
    });
    let (_daemon, client) = serve(builder).await?;

    let mut queries = pin!(join_all(targets.iter().map(|(uri, _)| client.get_diagnostics(Some(uri.clone())))));
    tokio::select! {
        _ = join_all(targets.iter().map(|(_, gate)| gate.reached())) => {}
        result = &mut queries => return Err(format!("diagnostics returned before publication: {result:?}").into()),
    }
    fake_request(&client, "fake/releaseDiagnostics").await?;

    for (result, (uri, _)) in queries.await.into_iter().zip(&targets) {
        let diagnostics = result?;
        assert_eq!(
            diagnostics.iter().map(|params| (&params.uri, params.diagnostics.len())).collect::<Vec<_>>(),
            [(uri, 1)]
        );
    }
    Ok(())
}
