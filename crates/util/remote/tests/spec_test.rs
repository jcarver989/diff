use clankerdiff_remote::{
    Acp, AcpCommand, AcpTarget, AcpTransport, Action, Lifecycle, RemoteError, RemoteSpec, RemotesFile,
};
use serde_json::json;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn a_remotes_file_names_each_remote_with_one_command_per_action() -> TestResult {
    let file = parse(&json!({ "remotes": [
        {
            "name": "patchwave",
            "dir": "/work/patchwave/clankers",
            "vars": { "CFG": "clankervm.toml" },
            "list": ["clankervm", "--config", "${CFG}", "remote", "list"],
            "stop": ["clankervm", "--config", "${CFG}", "stop", "${ID}"],
            "acp": { "websocket": ["clankervm", "connect", "${ID}", "aether", "--print-endpoint"] },
        },
        { "name": "devbox", "shell": ["ssh", "-t", "devbox"], "acp": { "stdio": ["ssh", "-T", "devbox", "aether", "acp"], "cwd": "/home/me/code" } },
    ]}))?;

    let [patchwave, devbox] = file.remotes.as_slice() else { panic!("two remotes") };
    assert_eq!(patchwave.name, "patchwave");
    assert!(!patchwave.is_static());
    assert!(patchwave.can(Action::List) && patchwave.can_change(Lifecycle::Stop));
    assert!(!patchwave.can(Action::Launch) && !patchwave.can_change(Lifecycle::Suspend));
    assert!(matches!(patchwave.acp, Some(Acp { transport: AcpCommand::Websocket(_), cwd: None })));
    assert!(devbox.is_static(), "a remote without `list` is static");
    assert!(devbox.can(Action::Shell));
    assert!(matches!(
        &devbox.acp,
        Some(Acp { transport: AcpCommand::Stdio(_), cwd: Some(cwd) }) if cwd == "/home/me/code"
    ));
    Ok(())
}

#[test]
fn a_command_expands_its_variables_and_runs_in_the_remotes_directory() -> TestResult {
    let remote = RemoteSpec {
        dir: Some("/work/patchwave".into()),
        vars: [("CFG".to_owned(), "clankervm.toml".to_owned())].into(),
        env: [("PROFILE".to_owned(), "${NAME}-profile".to_owned())].into(),
        stop: Some(argv(&["clankervm", "--config", "${CFG}", "stop", "$ID", "--in", "${NAME}", "$$HOME"])),
        ..RemoteSpec::named("patchwave")
    };

    let stop = remote.invocation(Action::Stop, Some("mvm-1"))?.ok_or("the remote can stop")?;

    assert_eq!(stop.program, "clankervm");
    assert_eq!(stop.args, ["--config", "clankervm.toml", "stop", "mvm-1", "--in", "patchwave", "$HOME"]);
    assert_eq!(stop.dir, PathBuf::from("/work/patchwave"));
    assert_eq!(stop.env, [("PROFILE".to_owned(), "patchwave-profile".to_owned())].into());
    Ok(())
}

#[test]
fn a_tilde_starts_the_home_directory_in_paths_and_programs() -> TestResult {
    let home = std::env::home_dir().ok_or("a home directory")?;
    let remote = RemoteSpec {
        dir: Some("~/clankers".into()),
        shell: Some(argv(&["~/bin/remote-shell", "~/not-a-path"])),
        ..RemoteSpec::named("devbox")
    };

    let shell = remote.invocation(Action::Shell, None)?.ok_or("the remote has a shell")?;

    assert_eq!(PathBuf::from(&shell.program), home.join("bin/remote-shell"));
    assert_eq!(shell.args, ["~/not-a-path"]);
    assert_eq!(shell.dir, home.join("clankers"));
    Ok(())
}

#[test]
fn a_command_runs_in_the_home_directory_by_default() -> TestResult {
    let remote = RemoteSpec { shell: Some(argv(&["ssh", "devbox"])), ..RemoteSpec::named("devbox") };

    let shell = remote.invocation(Action::Shell, None)?.ok_or("the remote has a shell")?;

    assert_eq!(Some(shell.dir), std::env::home_dir());
    Ok(())
}

#[test]
fn an_action_the_remote_lacks_has_no_invocation() -> TestResult {
    let remote = RemoteSpec::named("devbox");

    assert_eq!(remote.invocation(Action::Logs, Some("devbox"))?, None);
    assert_eq!(remote.acp(Some("devbox"))?, None);
    Ok(())
}

#[test]
fn an_unset_variable_is_reported() {
    let remote = RemoteSpec { logs: Some(argv(&["logs", "${ID}"])), ..RemoteSpec::named("patchwave") };

    let error = remote.invocation(Action::Logs, None).expect_err("there is no instance id");

    assert!(matches!(error, RemoteError::Variable(ref name) if name == "ID"), "{error}");
}

#[test]
fn an_empty_command_is_reported() {
    let remote = RemoteSpec { shell: Some(Vec::new()), ..RemoteSpec::named("devbox") };

    let error = remote.invocation(Action::Shell, None).expect_err("there is no program");

    assert!(matches!(error, RemoteError::EmptyCommand { .. }), "{error}");
}

#[test]
fn an_agent_is_reached_over_stdio_or_where_a_command_says_in_its_cwd() -> TestResult {
    let stdio = RemoteSpec {
        vars: [("SRC".to_owned(), "/home/me/code".to_owned())].into(),
        acp: Some(Acp { cwd: Some("${SRC}/$ID".to_owned()), ..Acp::stdio(argv(&["ssh", "devbox", "aether acp"])) }),
        ..RemoteSpec::named("devbox")
    };
    let websocket =
        RemoteSpec { acp: Some(Acp::websocket(argv(&["endpoint", "${ID}"]))), ..RemoteSpec::named("patchwave") };

    let Some(AcpTarget { transport: AcpTransport::Stdio(stdio_command), cwd: stdio_cwd }) =
        stdio.acp(Some("devbox"))?
    else {
        panic!("a stdio agent")
    };
    let Some(AcpTarget { transport: AcpTransport::Websocket(endpoint), cwd: websocket_cwd }) =
        websocket.acp(Some("mvm-1"))?
    else {
        panic!("a websocket agent")
    };

    assert_eq!(stdio_command.args, ["devbox", "aether acp"]);
    assert_eq!(stdio_cwd.as_deref(), Some("/home/me/code/devbox"));
    assert_eq!(endpoint.args, ["mvm-1"]);
    assert_eq!(websocket_cwd, None);
    Ok(())
}

#[test]
fn an_agent_needs_exactly_one_transport() {
    for acp in [json!({ "cwd": "/home/me/code" }), json!({ "stdio": ["aether", "acp"], "websocket": ["endpoint"] })] {
        let error =
            parse(&json!({ "remotes": [{ "name": "devbox", "acp": acp }] })).expect_err("an agent is reached one way");

        assert!(error.to_string().contains("exactly one"), "{error}");
    }
}

#[test]
fn a_remote_has_no_workspace() {
    let error = parse(&json!({ "remotes": [{ "name": "devbox", "workspace": "~/code" }] }))
        .expect_err("`workspace` is not a field");

    assert!(error.to_string().contains("workspace"), "{error}");
}

#[test]
fn a_remote_has_no_cwd_of_its_own() {
    let error = parse(&json!({ "remotes": [{ "name": "devbox", "cwd": "/home/me/code" }] }))
        .expect_err("`cwd` belongs to `acp`");

    assert!(error.to_string().contains("cwd"), "{error}");
}

#[test]
fn a_missing_remotes_file_names_no_remotes() -> TestResult {
    let dir = tempfile::tempdir()?;

    assert_eq!(RemotesFile::read(&dir.path().join("remotes.json"))?, RemotesFile::default());
    Ok(())
}

#[test]
fn a_remotes_file_with_an_unknown_field_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("remotes.json");
    fs::write(&file, json!({ "remotes": [{ "name": "devbox", "agent": ["ssh"] }] }).to_string())?;

    let error = RemotesFile::read(&file).expect_err("`agent` is not a field");

    assert!(matches!(error, RemoteError::Parse { .. }), "{error}");
    Ok(())
}

fn parse(value: &serde_json::Value) -> Result<RemotesFile, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("remotes.json");
    fs::write(&file, value.to_string())?;
    Ok(RemotesFile::read(&file)?)
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}
