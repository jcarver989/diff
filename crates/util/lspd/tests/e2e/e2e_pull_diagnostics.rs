use crate::common::{
    TestResult, TestWorkspace, fake, fake_request, file_uri, serve, wait_for_workspace_diagnostics,
    workspace_error_count,
};
use clankerdiff_lspd::LanguageId;
use clankerdiff_lspd::testing::TestDaemonBuilder;

#[tokio::test]
async fn diagnostics_are_pulled_and_cancelled_pulls_are_retried() -> TestResult {
    let project = TestWorkspace::new()
        .file("index.ts", "const error = 1;\n")
        .file("retry.ts", "// cancel-first-pull\nconst error = 1;\n");
    let (_daemon, client) = serve(pull_diagnostics(&project)).await?;

    let diagnostics = client.get_diagnostics(Some(file_uri(&project.path("index.ts")))).await?;
    let messages: Vec<_> = diagnostics.iter().flat_map(|params| &params.diagnostics).map(|d| &d.message).collect();
    assert_eq!(messages, ["error token"]);

    let workspace = client.get_diagnostics(None).await?;
    assert_eq!(workspace_error_count(&workspace), 2, "{workspace:?}");
    Ok(())
}

#[tokio::test]
async fn server_refresh_request_re_pulls_dependent_files() -> TestResult {
    let project = dependent_files();
    let (_daemon, client) = serve(pull_diagnostics(&project)).await?;

    let initial = client.get_diagnostics(None).await?;
    assert_eq!((initial.len(), workspace_error_count(&initial)), (2, 0));

    project.write("b.ts", "export const error = 1;\n");

    let refreshed =
        wait_for_workspace_diagnostics(&client, |diagnostics| workspace_error_count(diagnostics) == 2).await?;
    let a_uri = file_uri(&project.path("a.ts"));
    let a_diagnostics = refreshed.iter().find(|params| params.uri == a_uri).ok_or("a.ts diagnostics")?;
    assert_eq!(a_diagnostics.diagnostics[0].message, "dependency error");
    Ok(())
}

#[tokio::test]
async fn file_changes_still_reach_a_server_that_re_registers_its_watchers() -> TestResult {
    let project = dependent_files();
    let (_daemon, client) = serve(pull_diagnostics(&project)).await?;
    assert_eq!(workspace_error_count(&client.get_diagnostics(None).await?), 0);

    fake_request(&client, "fake/reregisterWatchers").await?;
    project.write("b.ts", "export const error = 1;\n");

    wait_for_workspace_diagnostics(&client, |diagnostics| workspace_error_count(diagnostics) == 2).await?;
    Ok(())
}

fn dependent_files() -> TestWorkspace {
    TestWorkspace::new().file("a.ts", "// depends: b.ts\n").file("b.ts", "export const ok = 1;\n")
}

fn pull_diagnostics(project: &TestWorkspace) -> TestDaemonBuilder {
    fake(project.root(), LanguageId::TypeScript, &["--pull-diagnostics"])
}
