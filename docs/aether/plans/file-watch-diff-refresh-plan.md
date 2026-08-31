# File Watching and Live Diff Refresh Plan

Issue: #3 — "The app should watch for file changes and update the diff" (`size:L`)

## Overview

### Problem statement

Today every host takes **one** immutable snapshot of the working tree when a review starts:

| Host | Snapshot source | Refresh today |
| --- | --- | --- |
| `clankerdiff` TUI (`--ui=tui`) | `GitRepository::snapshot_with_sources` (local or via the session socket) | manual only: `r` key → `RepositoryAction::Refresh` |
| Desktop (`--ui=desktop`, `diff-gpui-desktop`) | `GitRepository::snapshot_with_sources` in `DesktopApp::discover` | manual only: `⌘/Ctrl+R` → `DesktopApp::reload` |
| Web (`diff-gpui-web`) | serialized `DiffDocument` pushed by the page host | host re-dispatches `diff-review-set-document` |

If a user (or an agent) edits a file while a review window is open, the rendered diff is stale
until someone presses the refresh shortcut. The issue asks the app to notice worktree changes and
re-render the diff on Desktop, Web, and TUI, and suggests the well-established `notify` crate.

### Architectural outcome

```text
diff-git
    WorktreeWatcher: notify-backed, debounced, `.git`-blind worktree change signals
                      (native only; keeps `notify` out of every wasm target)

clankerdiff (TUI)
    AutoRefresh: polls the watcher inside the existing crossterm event loop and replays
                 the existing `RepositoryAction::Refresh` path for both TUI placements

diff-gpui-desktop
    DesktopApp: owns a WorktreeWatcher, receives debounced signals on a GPUI background
                task, and refreshes the viewer in place (no full-screen "Loading…" flash)

diff-gpui-web
    documented host-push contract + `diff-review-repository-pending` command so a host
    can drive the exact same refresh UX (browsers cannot watch the filesystem)
```

### Success criteria and acceptance conditions

1. **Desktop**: with `clankerdiff review --ui=desktop <repo>` open, editing a file in an external
   editor makes the diff update within ~500 ms of the last write, without the window leaving the
   diff view, without the selected file changing unnecessarily, and with already-placed comments
   surviving (re-anchored or dropped) exactly as they do for `⌘/Ctrl+R` today.
2. **TUI, current placement** (`--tui-placement=current`): editing a file updates the rendered diff
   while the review stays usable (typing, comment drafts, navigation).
3. **TUI, external placement** (`--tui-placement=external`, the default): the attached client
   notices the edit and pulls a fresh document from the session server over the existing
   `RepositoryAction::Refresh` request.
4. **New, changed, and removed files** all refresh the diff (added untracked file, deleted file,
   rename, mode change).
5. **No-ops stay no-ops**: a signal that produces a byte-identical `DiffDocument` (for example the
   events our own `stage`/`discard` produce) must not reset scroll/selection, must not clear the
   comment draft, and must not flash a pending indicator for longer than the refresh itself.
6. **Web**: the browser shell keeps rendering host-pushed documents, exposes a pending command so a
   host can show the same "refreshing" state, and this contract is tested and documented.
7. **Degradation is graceful**: when watching cannot start (missing root, inotify watch limits,
   unsupported platform), the app logs once and keeps working with manual refresh; no panic, no
   exit.
8. `just verify` passes (`fmt-check check feature-check lint test wasm-check doc-check`), in
   particular `notify` must not enter any `wasm32` target.

## Technical Approach

### Decisions

1. **Use `notify` 8.x (`recommended_watcher`) and implement our own debounce.**
   `notify-debouncer-full`/`-mini` exist, but they add `walkdep`/`file-id` dependencies and their
   behavior is harder to assert on. Our debounce window is ~40 lines of pure, deterministic logic
   over `Instant`s that we can unit test, and the wrapper is a natural seam for a `notify::NullWatcher`
   test fake. `notify` is native-only (inotify/FSEvents/ReadDirectoryChangesW) which is why it is
   added **only** to `diff-git`, whose crate docs already state it "is not available in browser
   builds".
2. **Watch the whole worktree root recursively and filter, rather than watching individual paths.**
   Files enter and leave the diff constantly (new untracked files, deletions, renames), so a
   path-list watcher would need re-arming logic. `RecursiveMode::Recursive` on the canonical root is
   what editors do. Cost: watch limits on enormous monorepos — mitigated by graceful degradation
   (criterion 7) and by a documented `PollWatcher` fallback as a follow-up.
3. **Exclude everything under `<root>/.git`.** `git status` opportunistically rewrites the index
   stat cache, and our own `stage`/`commit`/`discard` write there; watching `.git` would create
   self-inflicted feedback. External staging is intentionally out of scope for v1 (see follow-ups).
4. **Debounce before signaling, coalesce on the host.** The watcher emits a signal after a 200 ms
   quiet window (capped at 1 s of total burst delay). Hosts additionally ignore signals while a
   load is already in flight, because the in-flight snapshot will observe the change anyway and the
   next signal would arrive if it did not.
5. **Recompute with the existing `snapshot_with_sources`, do not build an incremental differ.**
   It already handles scope, untracked files, renames, binary/oversize files, and the
   unstable-snapshot retry. A cheap `DiffDocument` equality check before installing keeps no-op
   refreshes invisible. Incremental re-diffing of only the changed paths is an explicit follow-up.
6. **Reuse `RepositoryAction::Refresh` as the single refresh verb.** The TUI already maps `r` and
   the desktop maps `⌘/Ctrl+R` onto it, and both session/local backends already turn it into
   "re-snapshot and return a document". Auto-refresh therefore reuses the *exact* code path as the
   manual key, which keeps comment reconciliation (`Review::reconcile`, content fingerprints) and
   view preservation (`ReviewSession::set_document`) identical.
7. **The watcher lives in the process that owns the terminal/window event loop**, not in the session
   server: pushing over the one-request-per-connection session protocol would require a protocol
   change, while the attached TUI client can watch locally and then *pull* with `Refresh`. The
   client learns the root from `DiffDocument.repo_root`, which the session server always sets to the
   canonical worktree root.
8. **Web = host-push.** Browsers cannot watch arbitrary files, and `crates/clankerdiff/src/args.rs`
   deliberately rejects `--port`/`--web-assets` (asserted in `rejects_unknown_command_and_values`),
   so we must not add a serving mode to the CLI. The deliverable is a documented, tested
   host contract plus the missing `pending` command.

### Key types and patterns

- **Test builder pattern**: reuse/extend `diff_core::testing::DocumentBuilder` for document
  fixtures, and extract the existing `Repo` builder from `crates/diff-git/tests/git_contract.rs`
  into `crates/diff-git/tests/support/mod.rs` (same layout as `diff-ratatui/tests/support`) so both
  contract test binaries share it.
- **Ports-and-adapters per host**: `WorktreeWatcher` is the only new "real" object; hosts talk to it
  through a cheap `Option`-based handle so a disabled watcher is represented by `None` and every
  code path stays testable without a filesystem.
- **Pure decision helpers** for the pieces that otherwise need a window/terminal
  (`classify_refresh`, `DebounceWindow`), so they can be unit tested without GPUI or crossterm.
- **Errors**: `thiserror` `WatchError` in `diff-git`; watcher failures are *never* fatal.

## Implementation Steps

Each step compiles and tests independently. Run `just fmt && just check && just test` after each.

### Step 1 — Add the dependency

1. Add to `[workspace.dependencies]` in the root `Cargo.toml`:
   `notify = "8.1.0"`.
2. Add to `crates/diff-git/Cargo.toml`: `notify.workspace = true` and
   `async-channel.workspace = true` (the signal channel, usable from both sync TUI polling and
   async GPUI).
3. Confirm `cargo tree -p diff-gpui-web --target wasm32-unknown-unknown` does not contain `notify`
   (it must not — `diff-git` is not in the wasm graph).

### Step 2 — `WorktreeWatcher` in `diff-git`

Create `crates/diff-git/src/watch.rs` and re-export it from `crates/diff-git/src/lib.rs`
(`mod watch; pub use watch::{WatchError, WatchOptions, WorktreeChange, WorktreeWatcher};`).

```rust
// crates/diff-git/src/watch.rs
use async_channel::{Receiver, Sender};
use notify::{Event, EventKind, RecursiveMode, Watcher as _};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(200);
pub const MAX_BURST_DELAY: Duration = Duration::from_millis(1_000);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchOptions {
    /// Quiet window that must elapse before a burst becomes a signal.
    pub debounce: Duration,
    /// Upper bound on how long one burst may delay its signal.
    pub max_burst_delay: Duration,
}

/// A coalesced worktree change. Paths are best-effort, relative to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeChange { pub paths: Vec<PathBuf> }

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("the watched path is not a directory: {path}")]
    InvalidRoot { path: PathBuf },
    #[error("could not watch the worktree: {0}")]
    Watch(#[source] notify::Error),
}
```

Behavior to implement:

1. `WorktreeWatcher::spawn(root: impl Into<PathBuf>) -> Result<Self, WatchError>` and
   `spawn_with(root, options)`. Validate `root.is_dir()`, then
   `notify::recommended_watcher(closure)` + `watcher.watch(&root, RecursiveMode::Recursive)?`.
2. The `EventHandler` closure forwards `notify::Result<Event>` into an
   `async_channel::unbounded` event channel with `try_send` (never blocks).
3. A named `std::thread` (`"diff-worktree-watch"`) runs the loop below and owns the
   `notify` watcher so it is dropped with the thread.
4. `WorktreeWatcher::changes(&self) -> &Receiver<WorktreeChange>` (async-channel receivers are
   `Clone`, so each host can clone one). `Drop` sets a shutdown flag, wakes the loop with a
   sentinel, and joins the thread.
5. **Filtering** (before debounce): drop `Err(_)`, drop events whose paths are *all* under
   `<root>/.git`, and drop `EventKind::Access(_)` except `AccessKind::Close(CloseWrite)` (reads must
   not refresh; a save-via-close still does).
6. **Debouncing** — extract the pure part so it is unit-testable:

   ```rust
   /// Pure debounce state machine; `Instant` is injected so tests are deterministic.
   struct DebounceWindow { options: WatchOptions, burst_start: Option<Instant> }

   impl DebounceWindow {
       /// Returns `Some(deadline)` while the burst is still arming.
       fn record(&mut self, now: Instant) -> Option<Duration /* sleep until */>;
       /// Returns true when the quiet window elapsed and the signal must fire.
       fn elapsed(&mut self, now: Instant) -> bool;
       fn reset(&mut self);
   }
   ```

   Loop shape: block on `recv_blocking_timeout(SHUTDOWN_POLL)` while idle; on the first event arm
   the window, then keep extending the deadline by `debounce` on each new event, capped at
   `burst_start + max_burst_delay`; when the deadline passes, send one `WorktreeChange` with the
   paths seen in the burst and reset.
7. `notify::Event` path normalization: strip the `root` prefix; keep the raw path if it is not
   under the root (some backends report renamed-away paths).

### Step 3 — Watcher contract tests

1. Extract the existing `Repo` builder from `crates/diff-git/tests/git_contract.rs` into a new
   `crates/diff-git/tests/support/mod.rs` (`#![allow(dead_code)]`, same style as
   `crates/diff-ratatui/tests/support/mod.rs`) and declare `mod support;` in `git_contract.rs`.
   Keep the builder's API identical; only move it.
2. New `crates/diff-git/tests/watch_contract.rs` using `support::Repo` (a plain `TempDir` is enough
   for most cases; `Repo` is needed only where a real repository matters):
   - `signals_a_worktree_edit` — write to a tracked file, expect one `WorktreeChange` containing
     that path within 5 s (use `WatchOptions { debounce: 50ms, .. }`).
   - `signals_created_and_removed_files` — create a new file and remove another; both produce a
     signal.
   - `ignores_git_internals` — write `<root>/.git/index`, assert
     `changes().recv_blocking_timeout(500ms)` times out.
   - `coalesces_a_burst_into_one_signal` — five rapid writes, then exactly one signal followed by a
     timeout.
   - `rejects_a_missing_root` — `WorktreeWatcher::spawn("/nonexistent")` is
     `Err(WatchError::InvalidRoot { .. })`.
3. Unit tests for `DebounceWindow` in `watch.rs` (`mod tests`): a single event fires after
   `debounce`; events inside the window extend it; `max_burst_delay` caps the extension; `elapsed`
   is false before the deadline.

### Step 4 — TUI auto-refresh (`clankerdiff`)

All changes are in `crates/clankerdiff/src/tui.rs`.

1. Add a small orchestration type next to `DiffReviewBackend`:

   ```rust
   /// Drives watcher-triggered refreshes from the blocking TUI loop.
   struct AutoRefresh {
       changes: Option<Receiver<WorktreeChange>>,
       in_flight: bool,
   }

   impl AutoRefresh {
       fn disabled() -> Self { Self { changes: None, in_flight: false } }
       /// Starts watching `root`; failures are logged to stderr and leave refresh disabled.
       fn watch(root: &Path) -> Self;
       /// Drains pending signals; true means "start a refresh now".
       fn poll(&mut self) -> bool;
   }
   ```

   `WorktreeWatcher` must be kept alive for the lifetime of the review: store it in the
   `AutoRefresh` (a field `watcher: Option<WorktreeWatcher>`) so `Drop` happens when the review
   ends.
2. Change `run_diff_review` to take the watch root:
   `fn run_diff_review(state, backend, watch_root: Option<&Path>)`.
   - `run_local` passes `Some(repository.root())`.
   - `attach` builds the watcher from the fetched document: `Path::new(&document.repo_root)`, only
     when that path is an absolute existing directory (`tui_placement=external` servers always send
     the canonical root); otherwise pass `None`.
3. Inside the existing loop, right before `event::poll(Duration::from_millis(100))`:

   ```rust
   if auto_refresh.poll() && !auto_refresh.in_flight {
       auto_refresh.in_flight = true;
       state.set_repository_pending();            // draws the pending indicator first
       apply_repository_action(&mut state, backend, RepositoryAction::Refresh);
       auto_refresh.in_flight = false;
   }
   ```

   The 100 ms poll timeout already rate-limits this to ~10 Hz, which pairs well with the 200 ms
   debounce.
4. Factor the body of the `RepositoryAction` arm out of `apply_event` into

   ```rust
   fn apply_repository_action(
       state: &mut DiffReviewState,
       backend: &mut dyn DiffReviewBackend,
       action: RepositoryAction,
   ) { /* set_repository_pending -> backend.apply -> set_document|set_snapshot|set_repository_error */ }
   ```

   and call it from both `apply_event` and the auto-refresh branch, so manual and automatic refresh
   are literally one code path.
5. **No-op guard**: in `apply_repository_action`, before applying a `ReviewUpdate`, compare with the
   current document and only install when it differs:

   ```rust
   if **state.document() != document { state.set_document(Arc::new(document)); }
   else { state.clear_repository_pending(); }   // new tiny helper, also used on errors
   ```

   Add `pub(crate) fn clear_repository_pending(&mut self)` to
   `crates/diff-ratatui/src/state.rs` setting `repository_status = Idle; prompt = None;
   mark_dirty()` — this is the only `diff-ratatui` change and keeps a no-op refresh from leaving a
   stuck "working…" indicator.
6. Add `diff-core` with `features = ["test-support"]` to `crates/clankerdiff` dev-dependencies so
   tests can build documents with `DocumentBuilder`.
7. Unit tests in `tui.rs` (`mod tests`) with a `FakeBackend` implementing `DiffReviewBackend`:
   - `auto_refresh_polls_only_when_a_signal_arrives` — inject a signal through a real
     `async_channel` pair, assert `poll()` flips once then stays false.
   - `auto_refresh_is_disabled_without_a_watcher` — `AutoRefresh::disabled().poll()` is always
     false.
   - `identical_refresh_documents_are_ignored` — backend returns the same document twice; the state
     keeps its selection and `repository_status` returns to `Idle`.

### Step 5 — Desktop auto-refresh (`diff-gpui-desktop`)

All changes are in `crates/diff-gpui-desktop/src/app.rs` (+ manifest).

1. Add `async-channel.workspace = true` to `crates/diff-gpui-desktop/Cargo.toml`.
2. New `DesktopApp` fields:
   `watcher: Option<WorktreeWatcher>`, `watch_task: Option<Task<()>>`,
   `installed_document: Option<Arc<DiffDocument>>`, `load_in_flight: bool`.
3. Add a refresh mode so automatic refreshes stay in place:

   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   enum RefreshMode {
       /// User-initiated: show the full-screen loading panel (current behavior).
       Interactive,
       /// Watcher-initiated: keep the viewer mounted and show its pending badge.
       Background,
   }
   ```

   `load()` takes the mode: `Interactive` sets `LoadState::Loading` as today; `Background` leaves
   `LoadState` untouched and instead calls
   `viewer.update(cx, |viewer, cx| viewer.set_repository_pending(true, cx))`.
4. Start watching once a repository is known — in `install_snapshot`, when `self.watcher.is_none()`
   and `self.repository` is `Some`:

   ```rust
   let root = repository.root().to_path_buf();
   match WorktreeWatcher::spawn(root) {
       Ok(watcher) => {
           let changes = watcher.changes().clone();
           self.watcher = Some(watcher);
           self.watch_task = Some(cx.spawn(async move |this, cx| {
               while let Ok(_change) = changes.recv().await {
                   if this.update(cx, |app, cx| app.auto_reload(cx)).is_err() { break; }
               }
           }));
       }
       Err(error) => eprintln!("clankerdiff: live refresh unavailable: {error}"),
   }
   ```

   `auto_reload` returns early when `self.load_in_flight` (criterion 4 in "Decisions") and otherwise
   calls `self.reload_with(RefreshMode::Background, cx)`.
5. In the load completion handler, classify instead of blindly installing:

   ```rust
   enum RefreshOutcome { Install, Skip, Transient, Failed(String) }

   fn classify_refresh(
       result: &Result<RepositorySnapshot, GitError>,
       current: Option<&DiffDocument>,
   ) -> RefreshOutcome
   ```

   - `Ok(snapshot)` whose `document` equals `current` → `Skip` (clear the pending badge, keep
     everything).
   - `Ok(snapshot)` → `Install` (`install_snapshot`, which already calls
     `viewer.set_snapshot` and clears pending/error).
   - `Err(GitError::UnstableSnapshot)` → `Transient`: keep the current document, clear pending,
     and let the next signal retry (live editing makes an occasional unstable capture expected).
   - any other `Err` → `Failed`: `viewer.set_repository_error(..)` for `Background` mode, and the
     existing `LoadState::Error` panel only for `Interactive` mode.
6. Record `installed_document` in `install_snapshot` so `Skip` is detectable, and set
   `load_in_flight = true`/`false` around the spawned operation.
7. Re-spawn the watch loop when the scope changes (`cycle_scope`) is **not** needed: the watcher is
   root-scoped, not scope-scoped. Only `install_snapshot` needs to update `installed_document`.
8. Unit tests in `app.rs`'s existing `mod tests` (plus `DocumentBuilder` through a
   `diff-core`/`test-support` dev-dependency, mirroring `diff-ratatui`):
   - `classify_refresh` table test covering `Install`, `Skip`, `Transient`, `Failed`.
   - `background_refresh_preserves_the_ready_state` — assert `load(…, Background)` does not set
     `LoadState::Loading` (pure field assertions on a `DesktopApp` constructed through a small
     `#[cfg(test)]` constructor that skips window/GPUI work, or by extracting the state transitions
     into a `RefreshState` helper struct that owns `state`, `installed_document` and
     `load_in_flight`).

### Step 6 — Web host contract (`diff-gpui-web`)

All changes are in `crates/diff-gpui-web/src/lib.rs` (+ tests).

1. Extend the crate-level docs with the refresh contract: the host owns change detection (server
   watcher, WebSocket, File System Access API, …) and re-dispatches
   `diff-review-set-document`; `diff-review-repository-pending` may be dispatched before
   recomputing so the viewer shows the same pending state as native hosts.
2. Add the missing command, mirroring `set_repository_error` exactly:
   - a `WebCommand::RepositoryPending` already exists — reuse it;
   - `install_string_command(&document, "diff-review-repository-pending", set_repository_pending)`;
   - `fn set_repository_pending(_: &str) -> Result<(), JsValue>` that sends
     `WebCommand::RepositoryPending`;
   - export it from the `wasm` module (`pub use wasm::{…, set_repository_pending, …}`).
3. Extend `crates/diff-gpui-web/tests/web.spec.ts`:
   - dispatch `diff-review-set-document` twice with two small fixtures (a one-line file, then an
     edited version), assert no `runtimeErrors` and that the canvas is still mounted;
   - dispatch `diff-review-repository-pending`, assert no `runtimeErrors`.
   Keep the fixtures inline (like `documentFixture`) rather than adding JSON files.
4. Run `just wasm-check` and `just web-test` (the latter needs `trunk`/Chromium; if unavailable in
   the environment, note it in the PR description).

### Step 7 — Documentation and changelog

1. Document the new behavior where users look for it:
   - `README.md`: one bullet under "Why should I use ClankerDiff?" — diffs update live while you
     edit.
   - `crates/diff-gpui-web/src/lib.rs` module docs: the host-push refresh contract (Step 6).
   - `crates/diff-git/src/lib.rs` module docs: mention worktree watching alongside Git operations.
2. Add `### Added` entries under `## [Unreleased]` in `crates/diff-git/CHANGELOG.md`,
   `crates/diff-ratatui/CHANGELOG.md`, `crates/diff-gpui-desktop/CHANGELOG.md` and
   `crates/diff-gpui-web/CHANGELOG.md` (release-plz style, matching the existing entries).
3. Verify the full gate one last time: `just verify`.

## Testing Plan

### Unit tests

| Test | Where | Asserts |
| --- | --- | --- |
| `DebounceWindow` single/extend/cap/reset | `diff-git/src/watch.rs` | one signal per burst, `max_burst_delay` honored |
| `WorktreeWatcher` root validation | `diff-git/tests/watch_contract.rs` | `InvalidRoot` for non-directories |
| `AutoRefresh::poll` transitions | `clankerdiff/src/tui.rs` | one refresh per signal, disabled stays false |
| identical-document refresh is a no-op | `clankerdiff/src/tui.rs` | selection kept, status returns to `Idle` |
| `classify_refresh` | `diff-gpui-desktop/src/app.rs` | Install / Skip / Transient / Failed |
| background refresh keeps `LoadState::Ready` | `diff-gpui-desktop/src/app.rs` | no full-screen loading flash |

### Integration tests (real objects, real `git`)

| Test | Where | Asserts |
| --- | --- | --- |
| `signals_a_worktree_edit` | `diff-git/tests/watch_contract.rs` | a real write produces one signal with the path |
| `signals_created_and_removed_files` | `diff-git/tests/watch_contract.rs` | add/remove both signal |
| `ignores_git_internals` | `diff-git/tests/watch_contract.rs` | `.git/index` writes are filtered |
| `coalesces_a_burst_into_one_signal` | `diff-git/tests/watch_contract.rs` | five writes → one signal |
| browser host refresh contract | `diff-gpui-web/tests/web.spec.ts` | two `set-document` events + pending event render without errors |

### End-to-end manual checks (must be run before opening the PR)

1. `just tui` in one terminal, edit a watched file from another editor → diff updates; comments
   placed before the edit are still attached to the right lines (or dropped when their content
   disappeared); the comment draft survives.
2. `CLANKERDIFF_TUI_COMMAND='...' cargo run -p clankerdiff -- review` (external placement) → same
   result, confirming the socket round trip.
3. `just desktop` → edit a file: diff updates in place, no loading panel; `⌘/Ctrl+R` still shows the
   interactive loading panel; add a new untracked file and delete a tracked one.
4. Introduce a no-op signal (`touch` an unchanged file) → no visual churn.
5. Simulate watcher failure (`ulimit -n` low, or watch a path with no permission) → app stays usable,
   warning printed once.

### Edge cases to verify explicitly

- Save storms from an editor that uses atomic replace (write temp + rename): must produce one
  signal per save burst, never a torn snapshot (`UnstableSnapshot` → `Transient`, not an error).
- A file whose content is reverted to its committed state: the file disappears from the diff; the
  selected file moves to a neighbor instead of panicking.
- Refresh while a comment draft is open, while the sidebar/drawer is focused, and while the theme
  picker is open in the TUI.
- Binary and >`MAX_SOURCE_FILE_BYTES` files edited during a session (they degrade exactly as on the
  initial snapshot).
- Repositories without `HEAD` (unborn) and with staged-only/unstaged-only scopes.
- `just wasm-check` and `just feature-check` still pass (`diff-ratatui`'s no-default-features builds
  must not gain a `diff-git` dependency — the `clear_repository_pending` addition is the only
  `diff-ratatui` change and touches no feature-gated code).

## Files to Modify/Create

| File | Change | Action |
| --- | --- | --- |
| `Cargo.toml` | add `notify = "8.1.0"` to `[workspace.dependencies]` | modify |
| `crates/diff-git/Cargo.toml` | add `notify.workspace = true`, `async-channel.workspace = true` | modify |
| `crates/diff-git/src/lib.rs` | declare `mod watch;` and re-export the watcher API; mention watching in the module docs | modify |
| `crates/diff-git/src/watch.rs` | `WorktreeWatcher`, `WatchOptions`, `WorktreeChange`, `WatchError`, `DebounceWindow` + unit tests | create |
| `crates/diff-git/tests/support/mod.rs` | shared `Repo` test builder moved from `git_contract.rs` | create |
| `crates/diff-git/tests/git_contract.rs` | `mod support;` instead of the inline `Repo` builder | modify |
| `crates/diff-git/tests/watch_contract.rs` | watcher contract tests | create |
| `crates/clankerdiff/Cargo.toml` | add `async-channel` dep; `diff-core` dev-dep with `test-support` | modify |
| `crates/clankerdiff/src/tui.rs` | `AutoRefresh`, `apply_repository_action`, watch root plumbing for `run_local`/`attach`/`run_diff_review` | modify |
| `crates/diff-ratatui/src/state.rs` | `pub(crate) fn clear_repository_pending` | modify |
| `crates/diff-gpui-desktop/Cargo.toml` | add `async-channel.workspace = true`; `diff-core` dev-dep with `test-support` | modify |
| `crates/diff-gpui-desktop/src/app.rs` | watcher fields, `RefreshMode`, `classify_refresh`, `auto_reload`, `installed_document`/`load_in_flight` | modify |
| `crates/diff-gpui-web/src/lib.rs` | `diff-review-repository-pending` command + export, documented host contract | modify |
| `crates/diff-gpui-web/tests/web.spec.ts` | host-push refresh tests | modify |
| `README.md` | live-refresh bullet | modify |
| `crates/{diff-git,diff-ratatui,diff-gpui-desktop,diff-gpui-web}/CHANGELOG.md` | `Unreleased → Added` entries | modify |

## Additional Notes

### Deliberate non-goals (follow-ups to spawn after this lands)

1. **Incremental re-diff**: recompute only the changed paths with `FileDiff::from_texts` and splice
   them into the existing `DiffSnapshot` instead of a full `snapshot_with_sources` round trip. Worth
   doing once profiling shows a large-repo refresh cost.
2. **Watch `.git/index` / `HEAD`** so externally-run `git add`/`git checkout`/`git commit` also
   refresh; requires suppressing the events our own commands generate (an RAII `pause()` guard on
   `WorktreeWatcher`).
3. **Non-blocking TUI refresh**: run `snapshot_with_sources` on a background thread (the socket
   round trip for external placement especially) and post the result into the crossterm loop, so the
   UI never blocks on Git.
4. **`notify::PollWatcher` fallback** when `recommended_watcher` fails (NFS/WSL/inotify limits),
   selected by an env var such as `CLANKERDIFF_WATCH_BACKEND=poll`.
5. **Web: File System Access API host** (`showDirectoryPicker` + `getFile()` polling) so a hosted
   page can watch a local directory without a backend — Chromium-only, so it stays a host concern.
6. **Markdown reviews**: watch the reviewed Markdown file for `clankerdiff markdown` (both TUI and
   desktop) using the same `WorktreeWatcher` with a non-recursive watch.

### Risks and mitigations

- **Watch limits / huge monorepos** → failures are non-fatal (criterion 7), warning once, manual
  refresh still available; PollWatcher follow-up.
- **Editor save patterns (atomic rename)** → debounce coalesces create+write+rename bursts; the
  `UnstableSnapshot` retry inside `snapshot_with_sources` already guards torn reads, and
  `classify_refresh` treats the remainder as transient.
- **UI churn from no-op refreshes** → document equality check on every host before installing.
- **Comment/draft loss** → unchanged from today: `ReviewSession::set_document` reconciles by content
  fingerprint and keeps the open draft; covered by the manual checks above.
- **Clippy pedantic (`-D warnings`)** → all new public items need `#[must_use]`/docs as the rest of
  the workspace does; the plan's signatures already follow that style.
