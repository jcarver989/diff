# Issue #85 — Plan: Branch-vs-Base Diff + Commit-by-Commit Paging

## Overview

### Problem statement

Today every surface (CLI, TUI, GPUI desktop, GPUI web, live protocol) can only
review **working-tree/index state vs `HEAD`** via `DiffScope::{Unstaged, Staged, Both}`
(`crates/diff-core/src/models/diff_scope.rs`,
`crates/diff-git/src/repository.rs::diff_args`).

Issue #85 asks for two related, read-oriented review workflows:

1. **Branch vs target-branch diff** — see the full diff of a feature branch vs a
   base branch (e.g. `main`), i.e. `git diff <merge-base(base,head)> <head>`.
2. **Commit-by-commit stepping** — list the commits on a branch
   (`git log base..head`) and page through them one at a time, the standard
   stacked/PR review workflow.

Both need a coherent UX across the TUI (`diff-ratatui`) and GPUI
(`diff-gpui` + desktop + web shells) with discoverable keyboard shortcuts, while
keeping the existing working-tree review (including stage/unstage/commit)
untouched.

### Success criteria / acceptance conditions

- [ ] `review --base <rev> [--head <rev>]` shows the cumulative branch diff;
      `--commit <rev>` (or stepping into a commit from the commit list) shows a
      single commit (`parent..commit`, `EMPTY_TREE` for a root commit).
- [ ] A commit list (`base..head`, oldest-first) is available in TUI and GPUI;
      user can move prev/next commit without losing queued review comments that
      still anchor (existing `Review::reconcile` semantics).
- [ ] Committed range/commit views are **read-only**: stage/unstage/commit/
      discard commands are disabled via the existing `ReviewCapabilities.repository`
      gate (no new capability plumbing shape — just set `repository: false`).
- [ ] TUI and GPUI keyboard shortcuts exist, are shown in help/shortcuts UI, and
      share the same `DiffReviewCommand` vocabulary in `diff-core`.
- [ ] Live protocol (`diff-protocol` + `diff-server` + `diff-client` + watcher)
      transports the new target; `LIVE_PROTOCOL_VERSION` is bumped (2 → 3).
- [ ] Integration tests per touched crate under `tests/` (mirroring `src/`
      layout) prove: range diff contents, single-commit diff, commit listing,
      rev validation errors, read-only capabilities, and prev/next navigation.
- [ ] `cargo test`, `cargo clippy`, `cargo fmt --check` pass; feature-graph
      constraints hold (`diff-git` pulls no UI/watcher deps, `diff-watch`
      pulls no renderer deps — see `justfile` `feature-check`).

### Non-goals

- No branch checkout/create/delete UI; no push/fetch; no interactive rebase.
- No commit-graph visualization; a flat oldest-first commit list suffices.
- No per-commit comment persistence across commits beyond the existing
  in-memory `Review` + `reconcile` behavior (comments stay attached to the
  currently viewed document; navigating marks un-anchorable comments outdated).

---

## Technical Approach

### High-level architectural decisions

1. **New first-class `DiffTarget`, keep `DiffScope` for the working tree.**
   Do **not** add a fourth `DiffScope` variant — `DiffScope` is deeply coupled
   to the 3-slot watcher cache (`scope_index` in
   `crates/diff-watch/src/repository_watcher.rs`), `scope.next()` cycling, CLI
   `--scope` parsing, and web `scope` strings. Instead introduce one new type
   in `diff-core` that *contains* the scope for the working-tree case:

   ```rust
   // crates/diff-core/src/models/revision.rs (new)
   pub struct RevisionSpec(String); // validated rev, e.g. "main", "HEAD~2", oid
   pub struct CommitSummary { pub oid: String, pub short_oid: String,
       pub author: String, pub date: String, pub subject: String }

   pub enum DiffTarget {
       WorkingTree(DiffScope),                    // today's behavior
       Range { base: RevisionSpec, head: RevisionSpec }, // cumulative branch diff
       Commit { oid: RevisionSpec },              // single commit
   }
   impl DiffTarget {
       pub fn working_scope(&self) -> Option<DiffScope>;
       pub fn is_read_only(&self) -> bool; // true for Range | Commit
   }
   ```

   This is the **one way** to select "what am I diffing" going forward:
   `DiffTarget` replaces bare `DiffScope` at every layer boundary
   (git, watcher, protocol, client, renderer state, CLI). `DiffScope::next()`
   stays for in-working-tree cycling only.

2. **Git semantics: merge-base for ranges, `parent..commit` for single commits.**
   - Range diff = `git diff <merge-base(base,head)> <head> --` where the base is
     resolved once via `git merge-base base head`. This gives PR-style
     "feature vs main" even when `main` has moved forward, and degenerates to
     two-dot behavior when `base` is an ancestor. Document this choice in the
     new module docs; it matches `git diff base...head` without relying on the
     three-dot shorthand's DWIM in plumbing.
   - Single commit = resolve `oid^{commit}` via `git rev-parse --verify`,
     resolve parent via `git rev-parse oid^` (fallback to `EMPTY_TREE`
     `4b825dc642cb6eb9a060e54bf8d69288fbee4904` for root commits, reusing the
     existing constant), then `git diff parent oid --`.
   - Commit list = `git log --reverse --format=<unit-sep>--format` with `%x00`
     record separators over `base..head`, capped (e.g. 500 commits) with a
     `TooManyCommits` error variant.
   - Rev validation helper `GitRepository::resolve_commit(rev) -> String(oid)`
     using `git rev-parse --verify --quiet <rev>^{commit}`; new `GitError`
     variants `UnknownRevision(String)` and reuse `CommandFailed` otherwise.
     Ref/branch listing for a future picker: `git for-each-ref
     --format=%(refname:short)%00%(objectname:short)%00%(subject) refs/heads/`
     behind `GitRepository::list_branches()`. Ship the listing API in this
     change only if the picker UI needs it (TUI/GPUI steps below assume a
     minimal `B`-key base cycler first, full picker as stretch — see step 8).

3. **Source capture generalizes from HEAD-blobs to rev-blobs.**
   Today's `resolve_head_blobs` hardcodes `git ls-tree -r -z HEAD`. Generalize
   to `resolve_rev_blobs(rev: &str, paths)`, and generalize `ContentLocation`
   so range/commit sides resolve to `Blob(oid)` via `CatFileBatch` (already
   exists). Working-tree scopes keep the exact current
   `Absent/Head/Index/Worktree` mapping (`content_location()` unchanged).
   Range/commit snapshots skip `status --porcelain` and untracked-file reads
   entirely (nothing is untracked in a committed view) and always run the
   two-phase stable-snapshot protocol (`try_snapshot_with_sources`) since refs
   can move concurrently.

4. **Watcher becomes target-aware; committed snapshots are immutable.**
   `RepositoryWatcher`/`RepositoryActor` currently key everything off
   `DiffScope` with `[Option<...>; 3]` subscriptions. Change the actor to own
   `target: DiffTarget` plus a small bounded cache keyed by `DiffTarget`
   (active target always resident; keep the 3 working-tree scopes hot as today
   for `S`-cycling; evict non-active range/commit entries LRU, cap ~8).
   Filesystem-watch refresh (`should_refresh`) only re-resolves working-tree
   targets; for `Range`/`Commit` targets only `.git` ref/info events trigger a
   re-resolve (new commits landing on the branch update the view) plus explicit
   `Refresh`. This keeps the actor pattern (channels, no `Mutex`) intact.

5. **Protocol bump, single migration.**
   `LIVE_PROTOCOL_VERSION: 2 -> 3`. `DiffSnapshot { scope }` becomes
   `DiffSnapshot { target: DiffTarget, scope: DiffScope }` where `scope` is
   retained **one release** as a derived convenience (`target.working_scope()`
   or `Both` for read-only views) so web hosts sending bare documents keep
   working; `ClientCommand::Initialize { scope }` / `SetScope(DiffScope)` gain
   `SetTarget(DiffTarget)` siblings, with `SetScope` mapped to
   `SetTarget(WorkingTree(scope))` server-side. `ReviewCompletion` similarly
   carries `target` alongside `scope`. This is the minimal wire change that
   honors "one way to do something" on the Rust side while not bricking
   checked-in web fixtures (`demo-document.json`) on day one. Remove the
   deprecated `scope` fields in a follow-up (recorded below).

6. **Capabilities reuse, no new flags.**
   Committed views set `repository: false, refresh: true, scope: true`
   (via `diff-protocol/src/client/mod.rs::capabilities`, extended with a
   `target: &DiffTarget` parameter). `DiffReviewCommand::enabled` already gates
   stage/commit/discard/refresh/scope on these flags, so both renderers get
   read-only behavior for free. `submit`/`clipboard` stay document-gated as
   today so comments can still be submitted/copied on a range view.

7. **UI: commit strip + prev/next, shared command vocabulary.**
   New `DiffReviewCommand` variants (in `diff-core/src/commands.rs`):
   `SetTarget(DiffTarget)`, `NextCommit`, `PreviousCommit`
   (direct-index `SelectCommit(usize)` is optional stretch; prev/next covers
   the review workflow and keeps keybinding tables small).
   - TUI (`diff-ratatui`): `DiffReviewState` gains `target: DiffTarget`,
     `commits: Vec<CommitSummary>`, `selected_commit: Option<usize>`; drawer
     renders a top "Commits (n)" section above files; header/footer shows
     `feature → main · commit i/n · <short> <subject>`; keys `[` / `]`
     prev/next commit, `B` toggle back to working tree (emits
     `SetTarget(WorkingTree(last_scope))`). Keep `S` for scope cycling, enabled
     only in `WorkingTree` mode.
   - GPUI (`diff-gpui` `DiffViewer`): same state fields; new actions
     `NextCommit`/`PreviousCommit`/`BackToWorkingTree` bound to `[`, `]`, `B`
     in `bind_keys`; sidebar gains a commits section; header bar gains
     prev/next buttons + commit subject label; shortcuts modal gains a
     "Branch" section. Desktop `DesktopApp` forwards `SetTarget` like
     `SetScope` today; menus gain target items; web shell envelope gains
     `target` with `scope` fallback.

### Design patterns employed

- **Renderer-neutral domain model** (`diff-core` owns `DiffTarget`,
  `CommitSummary`, commands; TUI/GPUI stay thin) — the established pattern.
- **Actor + channels** for the watcher (no `Mutex`/`Arc<Mutex>` per repo
  conventions); cache lives inside `RepositoryActor`.
- **Test builder pattern** (`RepoFixtureBuilder`, `DocumentBuilder`): extend
  `RepoFixtureBuilder` with branch/commit helpers rather than new fixtures.
- **`thiserror`** for the new `GitError::UnknownRevision` etc.

### Key trade-offs

| Option | Chosen | Why |
|---|---|---|
| Extend `DiffScope` vs new `DiffTarget` | New `DiffTarget` wrapping `DiffScope` | Avoids breaking the 3-way scope index, `next()` cycling, and every `match scope` exhaustively; read-only views are a different axis than staged/unstaged |
| `merge-base` vs literal `base..head` diff | `merge-base(base,head)..head` | PR semantics ("what did this branch change vs base"); document it |
| Separate commit-list RPC vs piggyback on snapshot | New `GitRepository::list_commits` + include list in snapshot metadata (`RepositorySnapshot { target, commits }`) | Renderers need list + index atomically with the document to avoid skew; snapshot is the one delivery vehicle |
| Full branch picker vs minimal keys first | Minimal: `[`/`]` + `B` + `--base/--head/--commit` flags; picker stretch | Issue demands keyboard shortcuts and both UIs; picker is large UX work better done after data flow lands |

---

## Implementation Steps

Each step is atomic, lands behind no flag (additive API + version bump), and
includes its tests. Work in order; steps 1–4 are backend, 5–8 are surfaces.

### 1. `diff-core`: `DiffTarget` + `CommitSummary` + commands

- Create `crates/diff-core/src/models/revision.rs`:
  `RevisionSpec::new(String) -> Result<Self, RevisionSpecError>` (reject empty,
  NUL bytes, leading `-`; cap length 256); `CommitSummary` (serde);
  `DiffTarget` enum as above with `working_scope()`, `is_read_only()`,
  `as_str_for_log()` helpers; `From<DiffScope> for DiffTarget`.
- Re-export from `models/mod.rs` + `lib.rs`.
- Extend `DiffReviewCommand` (`commands.rs`) with
  `SetTarget(DiffTarget)`, `NextCommit`, `PreviousCommit`; gate
  `NextCommit`/`PreviousCommit` on `document_ready && phase == Browse`
  (no capability gate — navigation, not mutation); gate `SetTarget` like
  `SetScope` (`capabilities.scope && !repository_pending`).
- Extend `DiffReviewEvent` (`review.rs`) with `SetTarget(DiffTarget)`.
- Tests: `crates/diff-core/tests/models/revision_test.rs` (parse/validation,
  serde round-trip, `is_read_only`, command `enabled()` matrix).

### 2. `diff-git`: range/commit snapshots + commit listing

- Generalize `resolve_head_blobs(document)` → `resolve_rev_blobs(rev, document)`
  (`git ls-tree -r -z <rev> -- <paths>`); keep `resolve_index_blobs` as-is.
- Add `RepositorySnapshot { scope, target: DiffTarget, commits: Vec<CommitSummary>, document }`
  (keep `scope` derived for compat: working scope or `Both`).
- Add public API (one way — all take `DiffTarget`):
  - `snapshot_target(target) -> DiffDocument` (fast, no sources),
  - `snapshot_target_with_sources(target) -> RepositorySnapshot` (retry loop,
    same as `snapshot_with_sources` today),
  - `try_snapshot_target_with_sources(target)`,
  - `list_commits(base, head) -> Vec<CommitSummary>`,
  - `list_branches() -> Vec<BranchSummary>` (only if UI step needs it),
  - `resolve_commit(rev) -> String` (oid hex).
- Internals: `diff_args_for_target(target, has_head)` — `WorkingTree` delegates
  to existing `diff_args`; `Range` computes `merge_base` then
  `[diff, ..., <merge_base_oid>, <head>, --]`; `Commit` computes parent (or
  `EMPTY_TREE`) then `[diff, ..., <parent>, <oid>, --]`. `load_snapshot_input`
  takes `&DiffTarget`: skip `status`/untracked for read-only targets;
  `resolve_content_locations` takes `&DiffTarget` (rev-blobs for both sides on
  read-only; current worktree mapping otherwise).
- New `GitError` variants: `UnknownRevision(String)`, `AmbiguousRevision`,
  `TooManyCommits { count }`.
- Extend `RepoFixtureBuilder` (`testing.rs`): `.branch(name)`,
  `.commit_file(path, contents, message)`, `.checkout(rev)` helpers built on
  the existing `git()` runner.

### 3. `diff-watch` + `diff-server` + `diff-protocol` + `diff-client`

- `diff-core` re-export only; protocol:
  - `crates/diff-protocol/src/shared/document.rs`: `DiffSnapshot { target,
    scope /*derived, deprecated*/, document }`; `DocumentUpdate` gains
    `target: DiffTarget`.
  - `crates/diff-protocol/src/client/message.rs`: add
    `SetTarget(DiffTarget)`; keep `SetScope` (maps to working-tree target).
    Bump `LIVE_PROTOCOL_VERSION` to 3 (`shared/message.rs`).
  - `capabilities(connected, document)` → `capabilities_for(connected,
    document, target)` setting `repository: connected && !target.is_read_only()`.
- `diff-watch/src/repository_watcher.rs`: `RepositoryRequest::{RefreshScope,
  Subscribe, SetScope}` gain `SetTarget { target }` / take `DiffTarget`;
  `RepositoryActor { target: DiffTarget, ... }` with bounded target cache
  (`active: RepositoryState` + `working: [Option<State>; 3]` retained +
  `ranges: LruMap<DiffTarget, State>` or small `Vec` — prefer a tiny inline
  Vec<(DiffTarget, watch::Sender)> capped at 8, no new deps);
  `refresh_active()` re-resolves working-tree targets on worktree events and
  range/commit targets only on `.git` ref/info events (extend `filter.rs`
  with a `target_should_refresh(target, paths)` helper).
- `diff-server/src/connection.rs` + `server.rs`: handshake `Initialize`
  accepts `{ protocol_version: 3, target }` (accept `scope`-only hello from v2
  and map to `WorkingTree` during transition — or reject with
  `UnsupportedVersion`; **decide in code review, default to reject** since all
  first-party clients ship together); `start()` handles `SetTarget` like
  `SetScope`; `ReviewCompletion` carries `target`.
- `diff-client/src/state.rs` + `client.rs`: `ClientOptions { target:
  DiffTarget }` (keep `From<DiffScope>` impl mapping to `WorkingTree`);
  `handle(SetTarget)` sends `ClientCommand::SetTarget`.

### 4. CLI entry points (`clankerdiff` + desktop args)

- `crates/clankerdiff/src/args.rs`: `ReviewArgs` gains
  `--base <REV>`, `--head <REV> (default HEAD)`, `--commit <REV>`
  (`--commit` conflicts with `--base/--head`; `--base` without `--head`
  means `--head=HEAD`). Build `DiffTarget` from args; `ConnectArgs.scope`
  stays (mapped to `WorkingTree`) plus optional `--base/--head/--commit`
  triple. Update the three `args.rs` unit tests + add target-builder tests.
- `crates/diff-gpui-desktop/src/args.rs`: same triple (`--base/--head/
  --commit`), `CliArgs { target: DiffTarget }` (keep `scope` as derived
  accessor during transition); update `USAGE` text.
- `crates/clankerdiff/src/main.rs` + `tui.rs` + `diff-gpui-desktop/src/app.rs`
  `discover()`: open server, `subscribe(target)` / `ClientOptions::from(target)`.

### 5. TUI (`diff-ratatui`)

- `state.rs`: `DiffReviewState { target, commits, selected_commit,
  last_working_scope }`; `set_target()` mirrors `set_scope()` incl. deferred
  handling during prompts; `apply_client_state` installs `snapshot.target`.
- `diff_commands.rs`: dispatch `SetTarget` → emit `DiffReviewEvent::SetTarget`;
  `NextCommit`/`PreviousCommit` → compute neighbor OID from `commits` +
  `selected_commit` and emit `SetTarget(Commit{oid})` (host resolves + pushes
  new snapshot; selection index derived on install by OID match).
- `keybindings.rs` `default_diff_keybindings()`: `[` → `PreviousCommit`,
  `]` → `NextCommit`, `B` → back-to-working-tree (needs a parameterized
  command — use `SetTarget(WorkingTree(last_scope))` constructed at dispatch;
  bind `B` to a new `BackToWorkingTree` variant if parameterless binding is
  required by the `KeyBinding<DiffReviewCommand>` table — prefer adding
  `BackToWorkingTree` to keep the table declarative).
- `render.rs` + `drawer.rs`: commits section atop drawer (`Commits (n)`,
  `> short subject` selected row); header line
  `<head> → <base> · commit i/n · <short> <subject>` or
  `<head> → <base> · n commits` for range view; empty-commits → "No commits
  in range" empty state with hint to check `--base/--head`.
- Help/footer (`help_bindings`/`footer_hint` flow automatically from the
  keybinding table; verify `?` overlay lists the three new bindings).

### 6. GPUI shared component (`diff-gpui`)

- `viewer.rs` + new `viewer/commits.rs` (or extend `sidebar.rs`): same state
  fields as TUI; actions `NextCommit`, `PreviousCommit`, `BackToWorkingTree`;
  `bind_keys`: `[`, `]`, `B` under `BROWSE` context; `handle_command` mirrors
  TUI dispatch (emit `DiffViewerEvent::SetTarget`).
- `sidebar.rs`: commits section (file-list style rows, click selects commit);
  `diff_view.rs`: header bar with `‹`/`›` buttons + `commit i/n · subject`
  label + `working-tree` return affordance; `shortcuts.rs`: new "Branch"
  section (`[`/`]` step commit, `B` back to working tree, `S` scope only in
  working tree).
- `DiffViewerEvent` (`lib.rs`): add `SetTarget(DiffTarget)`; keep `SetScope`
  mapped at the host boundary.

### 7. Desktop + web shells

- `diff-gpui-desktop/src/app.rs`: `scope: DiffScope` → `target: DiffTarget`
  (+ derived scope for empty-state segments); `command()`/`handle_viewer_event`
  forward `SetTarget`; `menus.rs`: target menu items; empty-state scope
  segments hidden/disabled in read-only mode.
- `diff-gpui-web/src/lib.rs` + `commands.rs`: `DocumentCommand`/envelope gains
  optional `target` (serde; `scope` string retained as fallback mapping to
  `WorkingTree`); `SCOPE_REQUEST_EVENT` handling retained, add
  `TARGET_REQUEST_EVENT` (`diff-review-set-target`, JSON-serialized
  `DiffTarget`); `WebRoot::apply_command` installs `target` + commits;
  `demo-document.json` untouched (bare-document path still works).

### 8. Docs, help text, and polish

- `README.md`: review-workflow section (`--base/--head/--commit`, commit
  stepping keys, read-only note).
- Desktop `USAGE`, CLI `--help` (clap doc comments), TUI `?` overlay and GPUI
  shortcuts modal (covered in steps 5–6; this step verifies consistency).
- Decide branch-picker scope: if `list_branches()` shipped in step 2, add a
  minimal `B`-opens-picker vs `B`-toggles choice here; default
  recommendation: `B` toggles back to working tree, picker is follow-up.

### 9. Test hardening + rollout

- New integration tests (each crate `tests/` mirroring `src/`):
  - `diff-git/tests/range_test.rs`: branch fixture (main + feature with 3
    commits incl. rename + deletion) → range diff equals
    `merge-base..head` contents; single-commit diff incl. root commit;
    unknown rev → `UnknownRevision`; `list_commits` order/subject/empty-range.
  - `diff-watch/tests/target_test.rs`: subscribe working + range targets;
    worktree edit refreshes working target only; new commit on branch
    refreshes range target.
  - `diff-protocol/tests/target_test.rs`: v3 encode/decode incl. legacy
    `scope`-only fallback expectations.
  - `diff-ratatui/tests/target_test.rs`: `[`/`]` dispatch emits
    `SetTarget(Commit)`; `B` returns to working tree; read-only capabilities
    disable `ToggleStage`/`BeginCommit`.
  - `diff-gpui/tests/commits_test.rs`: same via `handle_command` +
    `command_enabled` matrix.
- Update existing tests touching `RepositorySnapshot { scope, document }`
  literals, `DiffSnapshot` construction, `ClientOptions::from(scope)`, and
  `scope_index` assumptions.
- Run: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`,
  `cargo fmt --check`, `just feature-check` (or equivalent lint target).

---

## Testing Plan

### Unit tests (new, colocated `#[cfg(test)]` only where crate convention exists)

- `RevisionSpec` validation; `DiffTarget::is_read_only` / `working_scope`;
  command `enabled()` gating for `SetTarget`/`NextCommit`/`PreviousCommit`
  under read-only vs working-tree capabilities.

### Integration tests (required — repo convention: public API only, `tests/`)

| Area | File | Cases |
|---|---|---|
| Core model | `crates/diff-core/tests/models/revision_test.rs` | validation, serde, gating matrix |
| Git ranges | `crates/diff-git/tests/range_test.rs` | range diff, single commit, root commit, unknown rev, commit list order/cap, merge-base semantics (advance base, view unchanged) |
| Watcher | `crates/diff-watch/tests/target_test.rs` | multi-target subscribe, selective refresh |
| Protocol | `crates/diff-protocol/tests/target_test.rs` | v3 round-trip, legacy fallback |
| TUI | `crates/diff-ratatui/tests/target_test.rs` | key dispatch, read-only gating, commit install/index |
| GPUI | `crates/diff-gpui/tests/commits_test.rs` | action dispatch, sidebar/header state |
| Web shell | `crates/diff-gpui-web/tests/commands_test.rs` (extend) | envelope with `target`, scope fallback |
| CLI args | `crates/clankerdiff/src/args.rs` tests (extend) + desktop `args.rs` tests | `--base/--head/--commit` parsing, conflicts |

Builders/fakes: extend `RepoFixtureBuilder` (branch/commit/checkout helpers)
and reuse `DocumentBuilder` for renderer tests — no bespoke fixtures.
Tests return `Result` and use `?` (no `unwrap` per repo convention, except
pre-existing fixture `expect`s).

### Edge cases to verify

- Empty range (base == head / up-to-date branch) → empty document + "No
  commits" state, submit still disabled-empty as today, no crash on `[`/`]`.
- Unknown/ambiguous rev, non-commit rev (e.g. tag to blob), `--commit` +
  `--base` conflict → clean CLI/GitError messages, UI error panel (not panic).
- Root commit (no parent) → diff vs `EMPTY_TREE`.
- Base moves forward mid-review → next ref-event refresh recomputes
  merge-base; queued comments reconcile/mark-outdated via existing logic.
- Binary/renamed/copied files inside ranges; large ranges (commit cap error).
- Detached HEAD / unborn repo + `--base` → `UnknownRevision` or sensible
  fallback (define in step 2, test it).
- Protocol mismatch (v2 client vs v3 server) → `UnsupportedVersion` error path.

### Manual verification

- TUI: `review --base main` on a fixture branch; `[`/`]` walk; `B` back;
  `?` overlay lists new keys; stage keys dead in range view.
- Desktop: same via `clankerdiff-gpui-desktop --base main`; header buttons.
- Web: push envelope with `target`, observe install + `document-applied` ack.

---

## Files to Modify/Create

| File | Change | Kind |
|---|---|---|
| `crates/diff-core/src/models/revision.rs` | New: `RevisionSpec`, `CommitSummary`, `BranchSummary`, `DiffTarget` + errors | Added |
| `crates/diff-core/src/models/mod.rs`, `lib.rs` | Re-export new types | Modified |
| `crates/diff-core/src/commands.rs` | `SetTarget`, `NextCommit`, `PreviousCommit`, (`BackToWorkingTree`) + gating | Modified |
| `crates/diff-core/src/review.rs` | `DiffReviewEvent::SetTarget` | Modified |
| `crates/diff-core/tests/models/revision_test.rs` | New integration tests | Added |
| `crates/diff-git/src/repository.rs` | `resolve_rev_blobs`, `*target*` snapshot APIs, `list_commits`, `list_branches`, `resolve_commit`, `diff_args_for_target`, `GitError` variants | Modified |
| `crates/diff-git/src/error.rs` | `UnknownRevision`, `TooManyCommits` | Modified |
| `crates/diff-git/src/testing.rs` | `branch`/`commit_file`/`checkout` builder helpers | Modified |
| `crates/diff-git/tests/range_test.rs` | New range/commit/listing tests | Added |
| `crates/diff-watch/src/repository_watcher.rs` | `DiffTarget` requests, actor cache, selective refresh | Modified |
| `crates/diff-watch/src/filter.rs` | `target_should_refresh` helper | Modified |
| `crates/diff-watch/tests/target_test.rs` | New watcher tests | Added |
| `crates/diff-protocol/src/shared/document.rs` | `DiffSnapshot.target` (+ deprecated `scope`), `DocumentUpdate.target` | Modified |
| `crates/diff-protocol/src/shared/message.rs` | `LIVE_PROTOCOL_VERSION` 2 → 3 | Modified |
| `crates/diff-protocol/src/client/message.rs` | `SetTarget`, handshake `target` | Modified |
| `crates/diff-protocol/src/client/mod.rs` | `capabilities_for(connected, document, target)` | Modified |
| `crates/diff-protocol/tests/target_test.rs` | New protocol tests | Added |
| `crates/diff-protocol/src/server/*` (event/completion types) | Carry `target` in completions | Modified |
| `crates/diff-server/src/server.rs` | `ReviewCompletion.target`, open/subscribe with target | Modified |
| `crates/diff-server/src/connection.rs` | Handshake + `SetTarget` handling | Modified |
| `crates/diff-client/src/state.rs`, `client.rs` | `ClientOptions.target`, `SetTarget` send | Modified |
| `crates/clankerdiff/src/args.rs` | `--base/--head/--commit` on review+connect, target builder | Modified |
| `crates/clankerdiff/src/main.rs`, `tui.rs` | Thread target through serve/connect/review | Modified |
| `crates/diff-gpui-desktop/src/args.rs` | `--base/--head/--commit`, `CliArgs.target`, `USAGE` | Modified |
| `crates/diff-gpui-desktop/src/app.rs`, `menus.rs` | Target state, forwarding, menus, empty-state | Modified |
| `crates/diff-ratatui/src/state.rs` | `target`/`commits`/`selected_commit`, install logic | Modified |
| `crates/diff-ratatui/src/diff_commands.rs` | `SetTarget`/`Next`/`Prev` dispatch | Modified |
| `crates/diff-ratatui/src/keybindings.rs` | `[`, `]`, `B` bindings | Modified |
| `crates/diff-ratatui/src/render.rs`, `drawer.rs` | Commits section, header/footer, empty states | Modified |
| `crates/diff-ratatui/tests/target_test.rs` | New TUI tests | Added |
| `crates/diff-gpui/src/viewer.rs`, `viewer/commands.rs` | Target/commit state, actions, dispatch | Modified |
| `crates/diff-gpui/src/sidebar.rs`, `diff_view.rs`, `shortcuts.rs`, `lib.rs` | Commits UI, header controls, help, `SetTarget` event | Modified |
| `crates/diff-gpui/tests/commits_test.rs` | New GPUI tests | Added |
| `crates/diff-gpui-web/src/lib.rs`, `commands.rs` | Envelope `target`, `TARGET_REQUEST_EVENT`, fallback | Modified |
| `README.md` | Branch/commit workflow docs | Modified |

Exact filenames under `tests/` follow the existing mirror convention
(`src/foo/boo.rs` → `tests/foo/boo_test.rs`); adjust to match each crate's
current layout when implementing (e.g. `diff-protocol` may need a new
`tests/` dir).

---

## Additional Notes

- **Documentation updates needed**: `README.md` workflow section; CLI `--help`
  text via clap doc comments; desktop `USAGE`; TUI `?` overlay and GPUI
  shortcuts modal update automatically from keybinding/action tables but must
  be eyeball-verified; consider a `docs/` branch-review guide as follow-up
  (no `docs/` dir exists today — out of scope for this change).
- **Protocol compatibility**: all first-party clients/servers ship from this
  monorepo, so a hard 2 → 3 bump with rejection of v2 hellos is acceptable;
  the only compat shim is the derived `scope` field + web `scope`-string
  fallback for external hosts. Remove those shims in a follow-up issue.
- **Follow-up tasks likely**: full branch picker UI (`B` opens ref list backed
  by `list_branches`); remote (non-local) branch support is already free via
  the server/client split; per-commit review persistence; range-aware
  `format_review` intro line ("reviewing commits A..B" vs working tree);
  web `connect_remote` default-target plumbing.
- **Risk watch**: `match` exhaustiveness on `DiffScope`/`ClientCommand`/
  `DiffReviewCommand` across renderers and tests — budget time for fallout;
  the watcher `[Option<_>; 3]` → target-cache conversion is the trickiest
  refactor, keep it inside the actor and covered by `target_test.rs` before
  touching UI steps.
- **Conventions reminder for implementer**: import types at file top (`Foo`,
  not `std::biz::Foo`); generics `T, U, V`; `thiserror` for errors; no
  `Mutex` — actor/channels only; one-way APIs (single `*target*` snapshot
  path, single `SetTarget` command); integration tests in `tests/` testing
  public API with `?`, builders over mocks, helpers at file bottom.
