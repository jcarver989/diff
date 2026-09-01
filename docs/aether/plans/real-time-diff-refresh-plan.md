# Real-Time Diff Refresh

## Overview

ClankerDiff currently loads a repository snapshot at startup and only reloads after an explicit repository action (including the manual `r`/⌘-R refresh). As a result, edits made by an agent or another editor while a review is open are not visible until the reviewer refreshes manually.

Add repository filesystem invalidation using the Rust `notify` ecosystem, then rebuild the existing full `RepositorySnapshot` after a short debounce. This must remain a host concern: `diff-core`, `diff-ratatui`, and `diff-gpui` stay repository-agnostic, while `diff-git` provides the reusable native watcher and the ClankerDiff TUI/GPUI desktop hosts consume it.

### Success criteria and acceptance conditions

- The native desktop diff app, an in-terminal TUI review, and an externally attached TUI review automatically show file creates, modifications, renames, and deletions made under the watched repository root.
- A burst produced by an editor's atomic-save sequence is debounced into a small number of snapshot reloads rather than one Git snapshot per raw filesystem event.
- The refreshed view is installed through `DiffReviewState::set_snapshot` / `DiffViewer::set_snapshot`, preserving the selected file/line and reconciling existing comments through the current `ReviewSession` behavior.
- A refresh never overlaps another snapshot/mutation load. If filesystem events arrive while a load is in flight, exactly one follow-up refresh is retained.
- Desktop refreshes retain the current rendered snapshot until the replacement is ready; they do not replace the entire app with the initial “Loading diff…” screen for every keystroke/save.
- Background read-only Git commands do not rewrite the index and trigger a watcher feedback loop.
- Watch setup/runtime failures use the existing repository-error/fallback path and do not remove manual refresh support.
- Markdown review and the hosted web frontend are unchanged; browser/server-side inputs do not have a native repository to watch.

## Technical Approach

### Shared watcher in `diff-git`

Create a small `RepositoryWatcher` abstraction in `diff-git`, backed by `notify`'s platform-selected `RecommendedWatcher` through `notify-debouncer-full`. The full debouncer is preferable to custom timers because it preserves `notify::EventKind`, understands common create/rename/remove save sequences, and lets the adapter ignore non-mutating `Access` events.

`RepositoryWatcher::new(root)` will:

1. Create a bounded `async_channel` carrying a host-neutral `RepositoryWatchEvent` (`Changed` or `Error(String)`). A capacity of one collapses additional invalidations while a host has not yet consumed the previous one.
2. Build a `notify_debouncer_full::Debouncer` with a named constant of roughly 200–250 ms and watch the canonical repository root with `RecursiveMode::Recursive`.
3. Convert any debounced batch containing `Any`, `Create`, `Modify`, or `Remove` into one `Changed` message; ignore pure `Access` events. Forward runtime watcher failures as `Error` without panicking.
4. Retain the debouncer as a field so dropping `RepositoryWatcher` stops the watcher thread. Expose a cloned receiver for async hosts and a nonblocking drain/poll method for synchronous hosts.

The event is only an invalidation. Do not transport patches or attempt to infer a `DiffDocument` from event paths. The authoritative response remains `GitRepository::snapshot_with_sources(scope)`, which already handles Git status, untracked files, source archives, and concurrent edits.

Watch the full repository root, including normal in-tree `.git` metadata, so external `git add`, reset, checkout, or commit operations can change staged/combined views as well as ordinary worktree edits. Document linked-worktree metadata outside the worktree root as a follow-up if it is not covered by the first implementation; do not recursively watch an arbitrarily large common object database in this change.

### Prevent watcher feedback from read-only Git

`git status` refreshes and writes cached index metadata by default. Once `.git` is watched, that can create an auto-refresh loop. Split the command helper into ordinary mutating `run(...)` and read-only `run_read(...)`; `run_read` sets `GIT_OPTIONAL_LOCKS=0`, as recommended by Git for background status/monitoring processes. Route discovery, diff/status, `rev-parse`, `ls-files`, and source resolution through `run_read`; keep stage/unstage/commit/discard commands on `run` so their requested mutations remain explicit.

### TUI integration

Keep both local and external TUI flows on the existing `DiffReviewBackend` abstraction:

- `run_local` starts a watcher for `GitRepository::root()`.
- `attach` obtains the repository root from the initial `DiffDocument::repo_root` and starts the watcher in the attached process.
- Pass the optional watcher into `run_diff_review`. Before/after each 100 ms crossterm poll, drain watcher messages. A `Changed` message calls a new shared helper equivalent to applying `RepositoryAction::Refresh`; it marks the state pending, invokes `backend.apply(Refresh)`, and installs `ReviewUpdate::Snapshot` or `ReviewUpdate::Document`.
- Refactor `apply_event` to call that same helper so keyboard refresh, watcher refresh, local Git loading, and external session loading retain one code path.

For an external TUI, do not extend the Unix protocol: the child can watch the repository path and send the existing `RepositoryAction::Refresh` request to the parent `session::run` server. The parent remains the authoritative snapshot owner and replies with the replacement document exactly as it does for manual refresh.

### Desktop integration and load serialization

After `DesktopApp::discover` succeeds, create and retain a `RepositoryWatcher`, clone its receiver, and hold a GPUI `Task<()>` that awaits events. On `Changed`, update the entity on the GPUI thread and call `request_reload(cx)`.

Introduce a small, testable reload gate (for example `ReloadGate { in_flight, refresh_queued }`) and use it for discovery, refreshes, and repository mutations:

- Starting work marks the gate in flight.
- A watcher/manual refresh during a load only sets `refresh_queued = true`.
- Completion installs the successful snapshot or reports the error, clears in-flight state, then starts one queued reload if needed.
- Do not replace an in-flight task by assigning a second task to `load_task`; this prevents stale/out-of-order snapshots and avoids depending on GPUI task-drop semantics.

Separate initial discovery presentation from background refresh presentation. Initial discovery may use `LoadState::Loading`; once a viewer exists, `reload` keeps the current `LoadState::Ready`/`Empty` content on screen and only swaps in the replacement snapshot when complete. Repository mutations may continue using the viewer's existing pending indicator. A background refresh failure should retain the prior snapshot and use `DiffViewer::set_repository_error`; initial discovery failures continue using `LoadState::Error`.

Watcher construction/runtime failures are nonfatal: retain manual `r`/⌘-R refresh, report the problem once through the host's existing error/warning route, and avoid retry loops. A later explicit rediscovery may attempt to establish the watcher again.

## Implementation Steps

1. **Add watcher dependencies.**
   - Add workspace-pinned `async-channel`, `notify = "8.2.0"`, and `notify-debouncer-full = "0.7.0"` entries as needed (reuse the existing workspace `async-channel` entry rather than duplicating it).
   - Add the workspace dependencies used by `diff-git` and regenerate `Cargo.lock`.

2. **Implement the reusable repository watcher.**
   - Add `crates/diff-git/src/watch.rs` with `WATCH_DEBOUNCE`, `RepositoryWatchEvent`, `RepositoryWatcher`, and a `thiserror`-based `RepositoryWatchError` for startup/watch-registration failures.
   - Build the debouncer with `new_debouncer`, register `root` recursively, hold its guard, and route the callback into a bounded channel with nonblocking `try_send` so callback threads never wait on UI/Git work.
   - Add a pure `should_refresh(EventKind) -> bool` helper that accepts unknown/rescan-capable mutation events conservatively and rejects `Access`/watch-management noise.
   - Provide `receiver()` for GPUI and `drain()` (returning whether a change was observed, plus any runtime error) for the synchronous TUI.
   - Re-export the public watcher types from `crates/diff-git/src/lib.rs`; keep raw `notify` types out of host APIs.

3. **Make background snapshot reads side-effect-free.**
   - In `crates/diff-git/src/command.rs`, factor command construction so `run_read` sets `GIT_OPTIONAL_LOCKS=0` while `run` retains normal behavior.
   - In `crates/diff-git/src/repository.rs`, change repository discovery and every read-only Git operation used by snapshot creation (`diff`, `status`, `rev-parse`, `ls-files`, HEAD/index source resolution, and renamed-path lookup) to `run_read`.
   - Leave stage, unstage, commit, restore, clean, and other requested mutations on `run`.

4. **Route TUI watcher events through the existing refresh action.**
   - In `crates/clankerdiff/src/tui.rs`, create a watcher from `repository.root()` in `run_local` and from the initial document's `repo_root` in `attach`.
   - Extend `run_diff_review` to own an optional watcher for the lifetime of the terminal loop and drain it on every poll cycle, including cycles with no keyboard input.
   - Extract `apply_repository_action(state, action, backend)` from `apply_event`; both `DiffReviewEvent::RepositoryAction` and watcher invalidations call it with the existing `ReviewUpdate` handling.
   - On a watcher runtime error, retain the current document, surface the error without panicking, disable/drop the broken watcher to prevent repeated failures, and leave manual refresh available.
   - Keep `crates/clankerdiff/src/session.rs` and the protocol wire format unchanged: external attach sends the existing `Refresh` action when its local watcher fires.

5. **Add serialized, non-flickering desktop reloads.**
   - In `crates/diff-gpui-desktop/src/app.rs`, add fields retaining `RepositoryWatcher`, its GPUI receiver task, and a `ReloadGate` (or equivalent explicit `load_in_flight`/`reload_pending` flags).
   - Start/restart watching only after successful repository discovery, and have the GPUI task await receiver events and marshal `request_reload` back through `this.update(cx, ...)`.
   - Refactor `load`, `reload`, and `mutate` into one serialized completion path. Queue one refresh when another operation is active; after every success or failure, consume the queued bit and start the next snapshot.
   - Ensure watcher/manual reloads retain the existing viewer while loading. Use the full loading/error panels only before the first snapshot; use viewer repository errors for later failures.
   - Keep `install_snapshot` unchanged as the final replacement boundary so comments/selection continue to be reconciled by `DiffViewer::set_snapshot`.

6. **Document behavior and scope.**
   - Update the README to state that native diff reviews auto-refresh after repository changes, that manual refresh remains available, and that hosted web/Markdown review is not filesystem-watched.
   - Note the debounce/fallback behavior rather than promising individual raw-event delivery.

7. **Run focused and workspace validation.**
   - Format and lint the changed crates.
   - Run watcher and Git contract tests, ClankerDiff TUI unit tests, GPUI desktop tests, then the workspace test suite/check appropriate to this repository.
   - Manually exercise current-terminal TUI, external TUI attach, and desktop against the same temporary repository while a second process edits/stages/deletes files.

## Testing Plan

### Unit tests

- `diff-git/src/watch.rs`:
  - `should_refresh` accepts create/modify/remove/unknown/rescan-style events and rejects access-only events.
  - Callback/channel logic emits one host invalidation for a multi-event batch and does not block or enqueue unbounded duplicate invalidations.
  - Startup errors are represented by `RepositoryWatchError`, not strings or panics.
- `clankerdiff/src/tui.rs`:
  - Extend a reusable fake `DiffReviewBackend` to record actions and return replacement `ReviewUpdate`s.
  - Verify a watcher invalidation invokes exactly `RepositoryAction::Refresh`, installs the returned snapshot/document, and marks errors through `set_repository_error`.
  - Verify keyboard `r` and watcher invalidation share the same helper.
- `diff-gpui-desktop/src/app.rs`:
  - Test `ReloadGate` without opening a window: the first request starts, requests during a load coalesce, completion starts exactly one follow-up, and an idle completion leaves no work.
  - Verify a load error still releases the gate and services a queued refresh.

### Integration tests

- Add `crates/diff-git/tests/repository_watch.rs`, following the existing real-temp-repository style in `git_contract.rs`:
  - Establish the watcher before modifying a tracked file; wait with a bounded deadline and assert a `Changed` invalidation arrives.
  - Cover untracked creation, atomic rename-over-save, and deletion.
  - Run external `git add` and assert the watched metadata causes an invalidation for staged/combined scope in a standard worktree.
  - Perform a burst of writes and assert the bounded/debounced adapter does not accumulate one pending message per raw write.
  - Use generous deadlines and polling loops so tests fail rather than hang on slow CI.
- Keep existing `diff-git` mutation contracts running after the `run_read` split to prove stage/unstage/commit/discard still use writable Git operations.
- If the GPUI test harness can construct `DesktopApp` without platform-window dependencies, add a focused async test that edits a temp repository and observes the installed document. Otherwise keep filesystem delivery covered in `diff-git`, the reload gate covered as a pure unit, and verify the final host wiring manually.

### Manual/edge-case verification

- Save a file through an editor that writes in place and one that uses temporary-file + rename; both update within roughly one debounce interval plus Git snapshot time.
- Rapidly write the same file while a snapshot is running; the UI never displays an older result after a newer one and performs one eventual follow-up refresh.
- Create, rename, and delete untracked/tracked files; empty and non-empty app states transition correctly.
- Run `git add`, `git reset`, and `git commit` outside ClankerDiff; staged/unstaged/both scopes update appropriately in a normal worktree.
- Keep an inline draft/comment open across an unrelated edit; existing selection/comment reconciliation is preserved, while a draft anchored to changed content follows the existing stale-draft behavior.
- Cause `snapshot_with_sources` to report `UnstableSnapshot`; the prior view remains usable, a queued later event can retry, and manual refresh still works.
- Exhaust/deny watcher resources: startup does not panic, the error is reported once, and manual refresh remains functional.
- Close each UI and verify dropping the host stops watcher/receiver tasks without keeping the process alive.

## Files to Modify/Create

| Path | Change | Status |
|---|---|---|
| `Cargo.toml` | Add/pin the `notify`/debouncer workspace dependencies; reuse the existing `async-channel` dependency. | Modify |
| `Cargo.lock` | Record the new watcher dependency graph. | Modify |
| `crates/diff-git/Cargo.toml` | Add workspace watcher/channel dependencies. | Modify |
| `crates/diff-git/src/watch.rs` | Implement the debounced, bounded repository invalidation abstraction and event/error filtering. | Create |
| `crates/diff-git/src/lib.rs` | Declare the watch module and re-export host-facing watcher types. | Modify |
| `crates/diff-git/src/command.rs` | Add the `GIT_OPTIONAL_LOCKS=0` read-only command path. | Modify |
| `crates/diff-git/src/repository.rs` | Route discovery/snapshot queries through the read-only command helper. | Modify |
| `crates/diff-git/tests/repository_watch.rs` | Add real-filesystem watcher integration coverage. | Create |
| `crates/clankerdiff/src/tui.rs` | Own/drain watchers in local and attached TUI modes and share repository refresh application logic. | Modify |
| `crates/diff-gpui-desktop/src/app.rs` | Own the watcher/task, serialize refreshes/mutations, queue one follow-up, and keep existing snapshots visible during reload. | Modify |
| `README.md` | Document native auto-refresh, manual fallback, and excluded web/Markdown scope. | Modify |

No changes are expected in `diff-core`, `diff-ratatui`, `diff-gpui`, `clankerdiff/src/protocol.rs`, or `clankerdiff/src/session.rs`; their current replacement/action boundaries already support this feature.

## Additional Notes

- Relevant upstream documentation: [`notify` 8.x](https://docs.rs/notify/latest/notify/) recommends `RecommendedWatcher` and calls out editor-specific save behavior; [`notify-debouncer-full`](https://docs.rs/notify-debouncer-full/latest/notify_debouncer_full/) provides rename-aware event coalescing; Git documents [`GIT_OPTIONAL_LOCKS=0`](https://git-scm.com/docs/git#Documentation/git.txt-codeGITOPTIONALLOCKScode) specifically for background processes that should not refresh/write the index as a side effect.
- Native watcher backends can miss events on NFS, WSL-mounted Windows paths, containers, or when inotify limits are exhausted. Manual refresh is the deliberate fallback; a configurable `PollWatcher` can be a follow-up if real users need network-filesystem support.
- Linked Git worktrees may keep relevant index/ref metadata outside the watched worktree root. If external Git-operation refresh is required there, follow up by teaching `GitRepository` to expose the worktree Git dir/common dir and watch only the relevant metadata parents rather than recursively watching the shared object database.
- Do not add filesystem watching to reusable rendering crates or the web build. That would mix native repository I/O into components intentionally designed to consume immutable replacement snapshots.