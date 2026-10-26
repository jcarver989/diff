use clankerdiff_remote::{CommandRemotes, InstanceState, Lifecycle, RemoteError, RemoteSpec, Remotes, short_id};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn listing_reads_the_instances_that_have_not_ended() -> TestResult {
    let remote = TestRemote::new()?.list(
        r#"{"instances":[
            {"id":"mvm-1","state":"running","expires_at":"2030-01-01T00:00:00Z","image":"ignored"},
            {"id":"mvm-2","state":"suspended"},
            {"id":"mvm-3","state":"ended"}
        ]}"#,
    );

    let instances = CommandRemotes.list(&remote.spec)?;

    let states: Vec<(&str, InstanceState)> = instances.iter().map(|vm| (vm.id.as_str(), vm.state)).collect();
    assert_eq!(states, [("mvm-1", InstanceState::Running), ("mvm-2", InstanceState::Suspended)]);
    let expires_at = SystemTime::UNIX_EPOCH + Duration::from_hours(525_960);
    assert_eq!(instances[0].expires_at, Some(expires_at));
    assert_eq!(instances[1].expires_at, None);
    Ok(())
}

#[test]
fn a_listing_that_is_not_the_contract_is_reported() -> TestResult {
    let remote = TestRemote::new()?.list(r#"{"microvms":[]}"#);

    let error = CommandRemotes.list(&remote.spec).expect_err("there are no instances");

    assert!(matches!(error, RemoteError::Output { .. }), "{error}");
    Ok(())
}

#[test]
fn launching_reads_a_bare_id_or_a_json_one() -> TestResult {
    let bare = TestRemote::new()?.launch("printf 'mvm-7\\n'");
    let json = TestRemote::new()?.launch(r#"printf '{"id":"mvm-8"}'"#);
    let silent = TestRemote::new()?.launch("true");

    assert_eq!(CommandRemotes.launch(&bare.spec)?, "mvm-7");
    assert_eq!(CommandRemotes.launch(&json.spec)?, "mvm-8");
    let error = CommandRemotes.launch(&silent.spec).expect_err("nothing was printed");
    assert!(matches!(error, RemoteError::NoId { .. }), "{error}");
    Ok(())
}

#[test]
fn a_change_runs_its_command_for_the_instance_in_the_remotes_directory() -> TestResult {
    let remote = TestRemote::new()?;
    let spec = RemoteSpec { suspend: Some(sh("pwd -P > \"suspended-$1\"", &["${ID}"])), ..remote.spec.clone() };

    CommandRemotes.change(&spec, "mvm-1", Lifecycle::Suspend)?;

    let written = fs::read_to_string(remote.dir.path().join("suspended-mvm-1"))?;
    assert_eq!(Path::new(written.trim()), remote.dir.path().canonicalize()?);
    Ok(())
}

#[test]
fn a_change_the_remote_lacks_is_refused() -> TestResult {
    let remote = TestRemote::new()?;

    let error = CommandRemotes.change(&remote.spec, "mvm-1", Lifecycle::Resume).expect_err("there is no resume");

    assert!(matches!(error, RemoteError::Unsupported { .. }), "{error}");
    Ok(())
}

#[test]
fn a_failed_command_reports_the_last_line_it_printed_on_stderr() -> TestResult {
    let remote = TestRemote::new()?;
    let spec = RemoteSpec {
        list: Some(sh("echo listing >&2; echo 'AWS credential setup failed' >&2; echo >&2; exit 3", &[])),
        ..remote.spec.clone()
    };

    let error = CommandRemotes.list(&spec).expect_err("the command fails");

    assert_eq!(error.to_string(), "AWS credential setup failed");
    Ok(())
}

#[test]
fn a_missing_program_is_reported() -> TestResult {
    let remote = TestRemote::new()?;
    let spec = RemoteSpec { list: Some(vec!["no-such-remote-program".to_owned()]), ..remote.spec.clone() };

    let error = CommandRemotes.list(&spec).expect_err("there is no program");

    assert!(matches!(error, RemoteError::Spawn { .. }), "{error}");
    Ok(())
}

#[test]
fn a_command_sees_the_remotes_environment() -> TestResult {
    let remote = TestRemote::new()?;
    let spec = RemoteSpec {
        env: [("REMOTE_ID".to_owned(), "mvm-9".to_owned())].into(),
        launch: Some(sh("echo \"$$REMOTE_ID\"", &[])),
        ..remote.spec.clone()
    };

    assert_eq!(CommandRemotes.launch(&spec)?, "mvm-9");
    Ok(())
}

#[test]
fn a_short_id_is_the_last_four_characters() {
    assert_eq!(short_id("mvm-01jb9z3f2a"), "3f2a");
    assert_eq!(short_id("ab"), "ab");
}

struct TestRemote {
    dir: TempDir,
    spec: RemoteSpec,
}

impl TestRemote {
    fn new() -> Result<Self, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let spec = RemoteSpec { dir: Some(dir.path().to_owned()), ..RemoteSpec::named("patchwave") };
        Ok(Self { dir, spec })
    }

    fn list(mut self, json: &str) -> Self {
        self.spec.list = Some(vec!["printf".to_owned(), "%s".to_owned(), json.to_owned()]);
        self
    }

    fn launch(mut self, script: &str) -> Self {
        self.spec.launch = Some(sh(script, &[]));
        self
    }
}

fn sh(script: &str, args: &[&str]) -> Vec<String> {
    ["sh", "-c", script, "sh"].into_iter().chain(args.iter().copied()).map(str::to_owned).collect()
}
