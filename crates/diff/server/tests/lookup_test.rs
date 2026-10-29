mod support;

use clankerdiff_client::{DiffClient, DiffScope};
use clankerdiff_core::{
    DefinitionRequest, DefinitionTarget, DiffSide, Fingerprint, Lookup, LookupFailure, LookupReply, LookupRequest,
    LookupStatus, LookupUnavailable, RepoPath, Resolution, SourceDocument, SourceLocation, SourcePath,
    testing::{external, location, repo, target},
};
use clankerdiff_git::testing::{RepoFixture, RepoFixtureBuilder};
use clankerdiff_lspd::{
    LanguageId,
    testing::{TestDaemon, TestWorkspace},
};
use clankerdiff_server::ServerOptions;
use std::{error::Error, sync::Arc, time::Duration};
use support::{TestServer, WAIT};
use tokio::time::{sleep, timeout};

const COMMITTED: &str = "pub fn build() -> u32 {\n    1\n}\n";
const CHANGED: &str = "pub fn build() -> u32 {\n    helper() + total()\n}\n\nfn total() -> u32 {\n    2\n}\n";
const UTIL: &str = "pub fn helper() -> u32 {\n    1\n}\n";
const EXTERNAL: &str = "// the standard library\npub struct u32;\n";

#[tokio::test]
async fn names_resolve_inside_the_diff_inside_the_repo_and_outside_it() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start().await?;

    let Resolution { targets, hover, status, current_version } =
        lookup.definition(location("src/lib.rs", 2)?, 15).await?;
    assert_eq!(targets.iter().map(|target| target.location.clone()).collect::<Vec<_>>(), [target("src/lib.rs", 5)?]);
    assert!(hover.is_some_and(|hover| hover.contains("line=1; character=15")));
    assert_eq!((status, current_version), (LookupStatus::Complete, false));

    let Resolution { targets, .. } = lookup.definition(location("src/lib.rs", 2)?, 4).await?;
    assert_eq!(targets[0].location, target("src/util.rs", 1)?);
    assert_eq!(targets[0].preview.as_deref(), Some("pub fn helper() -> u32 {"));

    let Resolution { targets, .. } = lookup.definition(location("src/lib.rs", 1)?, 18).await?;
    let external = targets.first().ok_or("u32 should resolve")?.location.clone();
    let SourcePath::External { display, .. } = &external.path else {
        return Err("u32 should resolve outside the repository".into());
    };
    assert!(display.ends_with("sync.rs"), "{display}");
    assert_eq!(external.line_number, 2);
    assert_eq!(lookup.source(external.path).await?.text(), EXTERNAL);
    Ok(())
}

#[tokio::test]
async fn sources_outside_the_allowlist_are_refused() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start().await?;

    assert_eq!(lookup.source(repo("src/util.rs")?).await?.text(), UTIL);
    let unknown = external(99, "std · alloc/src/sync.rs");
    let error = lookup.source(unknown).await.err().ok_or("unknown ids are refused")?;
    assert!(error.contains("unknown external source 99"), "{error}");
    Ok(())
}

#[tokio::test]
async fn lookups_no_language_server_can_answer_say_why() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start().await?;

    let content = lookup.content("src/lib.rs")?;
    let old = DefinitionRequest {
        at: SourceLocation { side: DiffSide::Old, ..location("src/lib.rs", 1)? },
        column: 7,
        content,
    };
    let unavailable = |reason| Resolution::unavailable(reason);
    assert_eq!(lookup.answer(old).await?, unavailable(LookupUnavailable::NotInCurrentVersion));
    let text = lookup.definition(location("notes.txt", 1)?, 0).await?;
    assert_eq!(text, unavailable(LookupUnavailable::NoLanguageServer));
    Ok(())
}

#[tokio::test]
async fn a_reviewed_line_that_is_unchanged_on_disk_is_looked_up_in_the_current_version() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start().await?;
    lookup.stage_then_write(&format!("// edited after staging\n{CHANGED}")).await?;

    let Resolution { targets, status, current_version, .. } = lookup.definition(location("src/lib.rs", 2)?, 4).await?;
    assert_eq!(targets.iter().map(|target| target.location.clone()).collect::<Vec<_>>(), [target("src/util.rs", 1)?]);
    assert_eq!((status, current_version), (LookupStatus::Complete, true));
    Ok(())
}

#[tokio::test]
async fn a_reviewed_line_that_changed_on_disk_is_not_sent_to_the_language_server() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start().await?;
    lookup.stage_then_write(&CHANGED.replace("helper() + total()", "helper() * total()")).await?;

    let resolution = lookup.definition(location("src/lib.rs", 2)?, 4).await?;
    assert_eq!(resolution, Resolution::unavailable(LookupUnavailable::ChangedSinceReview));
    Ok(())
}

#[tokio::test]
async fn a_language_server_that_never_initializes_reports_a_timeout() -> Result<(), Box<dyn Error>> {
    let options = ServerOptions { lookup_timeout: Duration::from_millis(500), ..ServerOptions::default() };
    let lookup = LookupFixture::start_with(&["--wedge-on", "initialize"], options).await?;

    let resolution = lookup.definition(location("src/lib.rs", 2)?, 4).await?;
    assert_eq!(resolution, Resolution::failed(LookupFailure::TimedOut));
    Ok(())
}

#[tokio::test]
async fn a_hover_that_never_answers_keeps_the_definitions_without_waiting_out_the_timeout() -> Result<(), Box<dyn Error>>
{
    let lookup = LookupFixture::start_with(&["--wedge-on", "textDocument/hover"], ServerOptions::default()).await?;

    let Resolution { targets, hover, status, .. } = lookup.definition(location("src/lib.rs", 2)?, 4).await?;
    assert_eq!(targets.iter().map(|target| target.location.clone()).collect::<Vec<_>>(), [target("src/util.rs", 1)?]);
    assert_eq!((hover, status), (None, LookupStatus::Complete));
    Ok(())
}

#[tokio::test]
async fn a_definition_that_never_answers_still_returns_the_hover() -> Result<(), Box<dyn Error>> {
    let options = ServerOptions { lookup_timeout: Duration::from_secs(3), ..ServerOptions::default() };
    let lookup = LookupFixture::start_with(&["--wedge-on", "textDocument/definition"], options).await?;

    let Resolution { targets, hover, status, .. } = lookup.definition(location("src/lib.rs", 2)?, 4).await?;
    assert_eq!(targets, NO_TARGETS);
    assert!(hover.is_some_and(|hover| hover.contains("line=1; character=4")));
    assert_eq!(status, LookupStatus::Failed(LookupFailure::TimedOut));
    Ok(())
}

#[tokio::test]
#[ignore = "requires rust-analyzer on PATH"]
async fn rust_analyzer_resolves_std_names_into_the_sysroot() -> Result<(), Box<dyn Error>> {
    let repo = RepoFixtureBuilder::new()
        .file("Cargo.toml", "[package]\nname = \"smoke\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
        .file("src/lib.rs", "pub fn one() -> u32 {\n    1\n}\n")
        .committed()
        .build();
    repo.write("src/lib.rs", "use std::sync::Arc;\n\npub fn shared() -> Arc<u32> {\n    Arc::new(1)\n}\n");
    let _daemon = TestDaemon::builder(repo.root(), LanguageId::Rust).spawn()?;
    let fixture = TestServer::open(repo).await?;
    let client = fixture.client().await?;
    let snapshot = client.state().snapshot.clone().ok_or("snapshot")?;
    let request = DefinitionRequest::at(&snapshot, location("src/lib.rs", 4)?, 4)?;
    for _ in 0..120 {
        if let LookupReply::Definition { resolution, .. } = client.lookup(lookup(request.clone().into())).await
            && let Some(target) = resolution.targets.first()
        {
            let SourcePath::External { display, .. } = &target.location.path else {
                return Err(format!("Arc should resolve outside the repository: {target:?}").into());
            };
            assert!(display.starts_with("std · ") && display.ends_with("sync.rs"), "{display}");
            return Ok(());
        }
        sleep(Duration::from_millis(500)).await;
    }
    Err("rust-analyzer never resolved Arc".into())
}

const NO_TARGETS: [DefinitionTarget; 0] = [];

struct LookupFixture {
    client: DiffClient,
    fixture: TestServer,
    _daemon: TestDaemon,
    _external: TestWorkspace,
}

impl LookupFixture {
    async fn start() -> Result<Self, Box<dyn Error>> {
        Self::start_with(&[], ServerOptions::default()).await
    }

    async fn start_with(server_args: &[&str], options: ServerOptions) -> Result<Self, Box<dyn Error>> {
        let repo = RepoFixtureBuilder::new()
            .file("Cargo.toml", "[package]\nname = \"lookups\"\nversion = \"0.1.0\"\n")
            .file("src/lib.rs", COMMITTED)
            .file("src/util.rs", UTIL)
            .committed()
            .build();
        repo.write("src/lib.rs", CHANGED);
        repo.write("notes.txt", "helper\n");
        let external = TestWorkspace::new().file("sync.rs", EXTERNAL);
        let daemon = daemon(&repo, &external, server_args)?;
        let fixture = TestServer::open_with(repo, options).await?;
        let client = fixture.client().await?;
        client
            .subscribe()
            .wait_until(WAIT, |state| state.snapshot.as_ref().is_some_and(|snapshot| snapshot.files.len() == 2))
            .await?;
        Ok(Self { client, fixture, _daemon: daemon, _external: external })
    }

    async fn stage_then_write(&self, worktree: &str) -> Result<(), Box<dyn Error>> {
        self.fixture.repo.git(&["add", "src/lib.rs"]);
        self.fixture.repo.write("src/lib.rs", worktree);
        self.client.set_target(DiffScope::Staged).await?;
        let path = RepoPath::new("src/lib.rs")?;
        self.client
            .subscribe()
            .wait_until(WAIT, |state| {
                state.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.source(&path, DiffSide::New).is_some_and(|source| source.text() == CHANGED)
                })
            })
            .await?;
        Ok(())
    }

    fn content(&self, path: &str) -> Result<Fingerprint, Box<dyn Error>> {
        let snapshot = self.client.state().snapshot.clone().ok_or("snapshot")?;
        Ok(snapshot.source(&RepoPath::new(path)?, DiffSide::New).ok_or("a new-side source")?.content_id())
    }

    async fn definition(&self, at: SourceLocation, column: usize) -> Result<Resolution, Box<dyn Error>> {
        let snapshot = self.client.state().snapshot.clone().ok_or("snapshot")?;
        let request = DefinitionRequest::at(&snapshot, at, column)?;
        self.answer(request).await
    }

    async fn answer(&self, request: DefinitionRequest) -> Result<Resolution, Box<dyn Error>> {
        match timeout(WAIT, self.client.lookup(lookup(request.into()))).await? {
            LookupReply::Definition { resolution, .. } => Ok(resolution),
            LookupReply::Source { .. } => Err("a definition lookup answered with a source".into()),
        }
    }

    async fn source(&self, path: SourcePath) -> Result<Arc<SourceDocument>, String> {
        match timeout(WAIT, self.client.lookup(lookup(LookupRequest::Source(path)))).await {
            Ok(LookupReply::Source { result, .. }) => result,
            Ok(LookupReply::Definition { .. }) => Err("a source lookup answered with a definition".to_owned()),
            Err(elapsed) => Err(elapsed.to_string()),
        }
    }
}

fn daemon(repo: &RepoFixture, external: &TestWorkspace, server_args: &[&str]) -> Result<TestDaemon, Box<dyn Error>> {
    let root = repo.root().canonicalize()?;
    Ok(TestDaemon::builder(repo.root(), LanguageId::Rust)
        .fake_server(server_args)
        .define("total", &root.join("src/lib.rs"), 5)
        .define("helper", &root.join("src/util.rs"), 1)
        .define("u32", &external.root().canonicalize()?.join("sync.rs"), 2)
        .spawn()?)
}

const fn lookup(request: LookupRequest) -> Lookup {
    Lookup { token: 0, request }
}
