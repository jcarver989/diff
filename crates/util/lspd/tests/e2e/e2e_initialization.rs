use crate::common::{TestResult, TestWorkspace, fake};
use clankerdiff_lspd::{ClientError, LanguageId};

#[tokio::test]
async fn servers_that_exit_or_reject_initialize_fail_connect_with_installation_help() -> TestResult {
    for args in [["--crash-on", "initialize"], ["--fail-on", "initialize"]] {
        let project = TestWorkspace::new();
        let daemon = fake(project.root(), LanguageId::TypeScript, &args).spawn()?;
        let error = daemon.client().await.err().ok_or("connect should fail")?;
        daemon.shutdown()?;
        let ClientError::InitializationFailed(message) = error else {
            return Err(format!("expected an initialization failure, got {error}").into());
        };
        assert!(message.contains("failed to complete initialization"), "{args:?}: {message}");
        assert!(message.contains("typescript@^7"), "{args:?}: {message}");
    }
    Ok(())
}
