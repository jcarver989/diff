mod support;

use clankerdiff_client::{DiffClient, DiffScope};
use clankerdiff_core::{
    DefinitionRequest, DiffSide, Fingerprint, Lookup, LookupReply, LookupRequest, RepoPath, Resolution, SourceDocument,
    SourceLocation, SourcePath,
    testing::{external, location, repo, target},
};
use clankerdiff_git::testing::{RepoFixture, RepoFixtureBuilder};
use clankerdiff_lspd::{
    LanguageId,
    testing::{TestDaemon, TestWorkspace},
};
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

    let Resolution { targets, hover } = lookup.definition(location("src/lib.rs", 2)?, 15).await?;
    assert_eq!(targets.iter().map(|target| target.location.clone()).collect::<Vec<_>>(), [target("src/lib.rs", 5)?]);
    assert!(hover.is_some_and(|hover| hover.contains("line=1; character=15")));

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
async fn names_the_language_server_cannot_answer_fall_back_to_matching_by_name() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start().await?;

    let content = lookup.content("src/lib.rs")?;
    let old = DefinitionRequest {
        at: SourceLocation { side: DiffSide::Old, ..location("src/lib.rs", 1)? },
        column: 7,
        content,
    };
    assert_eq!(lookup.answer(old).await?, Resolution::default());
    assert_eq!(lookup.definition(location("notes.txt", 1)?, 0).await?, Resolution::default());
    Ok(())
}

#[tokio::test]
async fn a_review_whose_code_differs_from_the_disk_is_not_sent_to_the_language_server() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start().await?;
    lookup.fixture.repo.git(&["add", "src/lib.rs"]);
    lookup.fixture.repo.write("src/lib.rs", format!("{CHANGED}// edited after staging\n"));
    lookup.client.set_target(DiffScope::Staged).await?;

    assert_eq!(lookup.definition(location("src/lib.rs", 2)?, 4).await?, Resolution::default());
    Ok(())
}

#[tokio::test]
async fn a_language_server_that_never_initializes_times_out_within_the_lookup_budget() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start_with(&["--wedge-on", "initialize"]).await?;

    assert_eq!(lookup.definition(location("src/lib.rs", 2)?, 4).await?, Resolution::default());
    Ok(())
}

#[tokio::test]
async fn a_hover_that_never_answers_keeps_the_definitions() -> Result<(), Box<dyn Error>> {
    let lookup = LookupFixture::start_with(&["--wedge-on", "textDocument/hover"]).await?;

    let Resolution { targets, hover } = lookup.definition(location("src/lib.rs", 2)?, 4).await?;
    assert_eq!(targets.iter().map(|target| target.location.clone()).collect::<Vec<_>>(), [target("src/util.rs", 1)?]);
    assert_eq!(hover, None);
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
    let request = DefinitionRequest::at(&snapshot, location("src/lib.rs", 4)?, 4).ok_or("a new-side name")?;
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

struct LookupFixture {
    client: DiffClient,
    fixture: TestServer,
    _daemon: TestDaemon,
    _external: TestWorkspace,
}

impl LookupFixture {
    async fn start() -> Result<Self, Box<dyn Error>> {
        Self::start_with(&[]).await
    }

    async fn start_with(server_args: &[&str]) -> Result<Self, Box<dyn Error>> {
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
        let fixture = TestServer::open(repo).await?;
        let client = fixture.client().await?;
        client
            .subscribe()
            .wait_until(WAIT, |state| state.snapshot.as_ref().is_some_and(|snapshot| snapshot.files.len() == 2))
            .await?;
        Ok(Self { client, fixture, _daemon: daemon, _external: external })
    }

    fn content(&self, path: &str) -> Result<Fingerprint, Box<dyn Error>> {
        let snapshot = self.client.state().snapshot.clone().ok_or("snapshot")?;
        Ok(snapshot.source(&RepoPath::new(path)?, DiffSide::New).ok_or("a new-side source")?.content_id())
    }

    async fn definition(&self, at: SourceLocation, column: usize) -> Result<Resolution, Box<dyn Error>> {
        let snapshot = self.client.state().snapshot.clone().ok_or("snapshot")?;
        let request = DefinitionRequest::at(&snapshot, at, column).ok_or("a new-side name in the snapshot")?;
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
