use clankerdiff_core::{CommitKind, CommitTitle, commit_description};

#[test]
fn conventional_subjects_split_into_kind_scope_and_a_capitalized_summary() {
    assert_eq!(
        CommitTitle::parse("feat(gpui): move paging into a diff toolbar"),
        title(Some(CommitKind::Feature), Some("gpui"), "Move paging into a diff toolbar", None)
    );
    assert_eq!(
        CommitTitle::parse("chore(deps): bump lru from 0.18.5 to 0.19.0 (#107)"),
        title(Some(CommitKind::Chore), Some("deps"), "Bump lru from 0.18.5 to 0.19.0", Some(107))
    );
    assert_eq!(
        CommitTitle::parse("fix!: drop the old flag"),
        title(Some(CommitKind::Fix), None, "Drop the old flag", None)
    );
    assert_eq!(
        CommitTitle::parse("Perf(core): cache rows"),
        title(Some(CommitKind::Performance), Some("core"), "Cache rows", None)
    );
}

#[test]
fn other_subjects_keep_their_text_and_only_lose_a_pull_request_suffix() {
    for subject in ["Uncommitted changes", "wip: try a thing", "feat:missing space", "Note: read this", "feat(: open"] {
        assert_eq!(CommitTitle::parse(subject), title(None, None, subject, None), "{subject}");
    }
    assert_eq!(CommitTitle::parse("Merge pull request (#42)"), title(None, None, "Merge pull request", Some(42)));
    assert_eq!(CommitTitle::parse("Mention (#x) in passing"), title(None, None, "Mention (#x) in passing", None));
}

#[test]
fn descriptions_drop_trailers_and_anything_after_a_fence() {
    assert_eq!(
        commit_description(
            "Why it matters\nacross lines.\n\nSecond paragraph.\n\nSigned-off-by: A <a@example.com>\nCo-authored-by: B <b@example.com>\n"
        ),
        "Why it matters\nacross lines.\n\nSecond paragraph."
    );
    assert_eq!(
        commit_description(
            "Bumps [lru](https://example.com) from 1 to 2.\n- [Changelog](https://example.com)\n\n---\nupdated-dependencies:\n- dependency-name: lru\n"
        ),
        "Bumps [lru](https://example.com) from 1 to 2.\n- [Changelog](https://example.com)"
    );
    assert_eq!(commit_description("Note: keep this line."), "Note: keep this line.");
    assert_eq!(commit_description("Signed-off-by: A <a@example.com>"), "");
    assert_eq!(commit_description("\n\n"), "");
}

fn title(kind: Option<CommitKind>, scope: Option<&str>, summary: &str, pull_request: Option<u64>) -> CommitTitle {
    CommitTitle { kind, scope: scope.map(str::to_owned), summary: summary.to_owned(), pull_request }
}
