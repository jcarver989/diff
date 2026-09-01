# Concurrent Diff Processing Architecture

## Overview

The current unstaged changes correctly treat native filesystem notifications as debounced repository invalidations and rebuild an authoritative `RepositorySnapshot`. However, the host implementations are not yet a safe foundation for concurrent processing:

- `diff-gpui-desktop::RepositoryWorker` serializes by awaiting each operation, but its command/update queues are unbounded and results have no generation identity.
- The local TUI performs Git reloads synchronously inside the terminal event loop.
- Tree-sitter/Arborium cache misses run inside GPUI and Ratatui render methods.
- CPU-heavy per-file `FileDiff::from_texts` work is serial inside `GitRepository::snapshot_with_sources`.
- A capacity-one watcher event channel may silently drop a fatal watcher error when a `Changed` event already occupies the slot.

Build a reusable native processing architecture around two deliberately small services:

1. A **Tokio repository actor** in `diff-git` owns repository discovery, the watcher, scope, mutations, and snapshot generations. It serializes stateful repository mutations, coalesces latest-wins invalidations/scope changes, and publishes only the newest completed immutable snapshot.
2. A **Rayon-backed syntax service** in `diff-syntax` executes demand-driven highlight jobs off UI threads. Each worker uses `arborium::Highlighter::fork()` so workers share loaded grammars but have independent parse contexts. Existing synchronous highlighting remains available for web/WASM and embedders.

Rayon is also used inside snapshot construction for independent CPU-heavy per-file diff derivation. Async filesystem reads are bounded separately and assembled deterministically. The architecture retains full immutable `DiffSnapshot` replacement as the consistency boundary; it does not attempt path-based incremental Git patches in this change.

### Success criteria and acceptance conditions

- Native desktop, current-terminal TUI, and attached/external TUI remain responsive while snapshots and syntax highlights are being computed.
- A filesystem change, manual refresh, scope change, or completed mutation increments a desired repository generation. A snapshot completion is installed only when its generation is still current; stale completions are discarded.
- Filesystem events received during a snapshot coalesce into exactly one eventual follow-up snapshot for the latest scope. The system does not build an unbounded queue of refresh work.
- Repository mutations execute FIFO and never overlap another mutation. Any read made obsolete by a mutation is cancelled where possible or ignored, and every successful mutation is followed by an authoritative snapshot before the mutation is considered settled.
- All cross-thread queues are explicitly bounded or have a proven structural bound. Queue-full and closed-service cases return typed errors instead of silently dropping user mutations.
- Fatal watcher failures are delivered reliably even when a change invalidation is already pending; auto-refresh is disabled once, the last snapshot remains usable, and manual refresh continues to work.
- Independent changed-file diffs are processed in parallel while final file ordering and snapshot content remain deterministic and byte-for-byte equivalent to the serial implementation.
- Native syntax cache misses run on Rayon rather than GPUI/Ratatui render threads. Duplicate requests for the same cache key are coalesced, obsolete theme/document results are rejected by key/revision, and completion triggers a redraw.
- Existing web/WASM behavior and public synchronous renderer APIs continue to compile and work without native threads.
- Dropping/shutting down a host closes clients, watcher resources, dispatcher tasks, and Git child processes without keeping the application alive.
- Existing selection, draft, and comment reconciliation still occurs through `ReviewSession::set_snapshot` / `DiffViewer::set_snapshot`.

## Technical Approach

### 1. Use an actor for ownership, not for CPU execution

Create `diff_git::RepositoryService` as a small hand-written Tokio actor rather than adding an actor framework. The actor is the single owner of:

- repository discovery and `GitRepository`;
- current `DiffScope`;
- `RepositoryWatcher` and watcher health;
- desired/published generation numbers;
- the active read-only snapshot task;
- a bounded FIFO of mutation requests;
- the last successful `Arc<RepositorySnapshot>` and current operation/error state.

Expose a cloneable `RepositoryClient` with nonblocking methods:

```rust
pub fn refresh(&self) -> Result<(), RepositoryServiceError>;
pub fn set_scope(&self, scope: DiffScope) -> Result<(), RepositoryServiceError>;
pub fn mutate(&self, action: RepositoryAction) -> Result<(), RepositoryServiceError>;
pub fn subscribe(&self) -> tokio::sync::watch::Receiver<RepositoryState>;
```

Use separate primitives according to semantics:

- a capacity-one invalidation signal for refreshes (coalescing is intentional);
- a Tokio `watch` value for scope (latest value wins);
- a bounded `mpsc` queue (for example, 32 entries) for user mutations, where `try_send` reports `Busy` or `Closed`;
- a Tokio `watch` channel for `RepositoryState`, because UI consumers need only the latest state, not an unbounded history.

`RepositoryState` should be clone-cheap and include `generation`, `scope`, `phase` (`Discovering`, `Loading`, `Mutating`, `Idle`), the latest `Option<Arc<RepositorySnapshot>>`, an optional nonfatal repository error, and an optional watcher warning. Keep the last successful snapshot present through background loading and errors.

The actor loop must continue selecting commands and watcher invalidations while a snapshot task is active. Tag each snapshot task with its starting generation. On completion:

```text
if completed_generation == desired_generation && no mutation is pending:
    publish snapshot and mark idle
else:
    discard completion and start one snapshot for desired_generation
```

Do not run mutations concurrently. When a mutation arrives, cancel/abort an obsolete read task, execute the mutation, increment the desired generation, drain watcher invalidations caused by that mutation, and start one post-mutation snapshot. Read-task abort is an optimization; generation checks are the correctness mechanism because CPU work already running on Rayon cannot be forcibly stopped.

This actor replaces the desktop-only worker and becomes the common repository-processing boundary for every native host.

### 2. Keep authoritative full snapshots, but parallelize safe internal stages

Preserve `GitRepository::snapshot_with_sources` and its existing two-attempt stability contract. The initial/final Git metadata comparison and worktree fingerprint validation remain mandatory.

Within one snapshot:

- Continue using one `git cat-file --batch-command` stream for Git object blobs; that protocol is stateful and should stay serialized.
- Partition worktree source reads from blob reads. Load worktree files concurrently with a named limit (for example `MAX_CONCURRENT_SOURCE_READS = 16`) using `futures::stream::buffer_unordered`.
- Store raw capture outcomes keyed by `SourceKey`, then classify UTF-8/binary content and apply the 64 MiB archive budget in original document/file/side order. This retains deterministic budget behavior regardless of completion order.
- After source capture and stability validation, move document derivation to the Rayon pool. Process each `FileDiff` independently with `FileDiff::from_texts`, collect through an indexed parallel iterator so the existing file order is retained, and preserve the current fallback to the Git patch representation when complete sources are unavailable or derivation fails.
- Bridge Rayon completion back into async code with a one-shot channel rather than running CPU-heavy loops directly on a Tokio worker or spawning an unconstrained `spawn_blocking` task per file.

Do not parallelize repository mutations or remove the stability pass. Do not infer affected files from watcher paths in this iteration; notify remains an invalidation source because Git status/scope, renames, index changes, and atomic saves make raw paths insufficient as authoritative input.

### 3. Make watcher change coalescing and failure delivery independent

Refactor `RepositoryWatcher` so `Changed` and fatal runtime failures do not compete for one capacity-one slot. Use a capacity-one changed receiver plus a separate one-shot/capacity-one fatal-error receiver (or equivalent atomically stored fatal state plus wakeup). The actor selects both sources. The synchronous `drain` compatibility API checks fatal state first and then drains the changed bit.

Once a fatal runtime error is observed, drop the debouncer/watcher, publish one warning, and retain manual refresh. Startup failures remain typed `RepositoryWatchError`s and are nonfatal to the service after repository discovery.

### 4. Use Rayon for CPU-bound work; do not use Tokio as the CPU pool

Add workspace-pinned `rayon` (the lockfile currently resolves 1.12.0) and `futures`. Rayon provides a fixed work-stealing pool suited to `FileDiff::from_texts` and Tree-sitter parsing. Tokio remains responsible for subprocesses, filesystem I/O, actor coordination, and UI channel waits.

Avoid Salsa in this change. Salsa is valuable for stable pure `K -> V` incremental query graphs, but repository capture here is side-effectful, has bounded-source/error semantics, and already crosses an immutable snapshot boundary. Define jobs and cache keys as pure owned values so Salsa or finer path-based incrementality can be introduced later if profiling justifies it.

### 5. Split syntax cache lookup from syntax execution

Extend `diff-syntax` with owned, thread-safe work/result types:

```rust
pub struct HighlightJob {
    pub key: CacheKey,
    pub theme: DiffTheme,
    pub language: String,
    pub source: Arc<str>,
    pub projection: HighlightProjection, // complete document or single source
}

pub struct HighlightResult {
    pub key: CacheKey,
    pub highlights: HighlightOutput,
}
```

Add `SourceDocument::text_shared() -> Arc<str>` in `diff-core` so complete-source jobs do not copy up to 8 MiB of text. Hunk fallback jobs may join their bounded (maximum 512-line) sequence into an `Arc<str>`.

Refactor `SyntaxHighlighter` into two responsibilities while retaining its current synchronous API:

- `lookup_*` computes the same theme/language/content `CacheKey`, returns cached output when present, and records a key in a `pending` set when it produces a job.
- `execute_job` parses owned input with a worker highlighter and returns `HighlightResult`.
- `install_result` removes the pending marker and inserts a result only if its key/theme revision is valid for the current cache.
- Existing `highlight_document`, `highlight_document_lines`, and `highlight_source` call lookup and execute synchronously, preserving web/embedding compatibility and existing statistics contracts.

Create a feature-gated native `ParallelSyntaxService` in `diff-syntax`:

- bounded job input (for example, 64 jobs);
- a dispatcher that keeps at most `rayon::current_num_threads()` jobs in flight;
- one forked Arborium highlighter per in-flight job, sharing the grammar store through `Highlighter::fork()`;
- structurally bounded result delivery (the unconsumed result count cannot exceed queued plus in-flight jobs);
- duplicate suppression in the UI-owned `SyntaxHighlighter::pending` set;
- explicit close/shutdown behavior.

The service is demand-driven by visible cells. Do not eagerly parse every source in a 64 MiB snapshot; when split view requests old and new sides, those two documents can execute concurrently, and navigation naturally requests later files.

### 6. Add background highlighting as an adapter option

Add a renderer-neutral `HighlightJobSink: Send + Sync` trait in `diff-syntax`. Extend `DiffViewerOptions` and `DiffReviewState` configuration with a mode equivalent to:

```rust
enum HighlightExecution {
    Synchronous,
    Background(Arc<dyn HighlightJobSink>),
}
```

Because `DiffViewerOptions` currently derives `Copy`, change it to `Clone` when it gains the sink. Keep `Synchronous` as the default so reusable components and web behavior are unchanged.

In background mode, `highlight_cell` / `cell_highlights` must:

1. return cached spans immediately on a hit;
2. submit a deduplicated owned job on a miss;
3. render plain themed text while pending instead of parsing inline;
4. remove the pending marker if the bounded sink rejects submission, allowing a later frame to retry.

Native hosts own the result receiver. They drain results, call `SyntaxHighlighter::install_result`, and notify/mark dirty only when an accepted result affects the current theme/document. Content-derived keys make late equivalent results reusable while preventing an old result from styling unrelated content.

### 7. Desktop integration

Remove the desktop-specific `repository_worker.rs` after moving its reusable responsibility into `diff-git`.

`DesktopApp` should own:

- `RepositoryServiceHandle`/`RepositoryClient`;
- a GPUI task that watches `RepositoryState` and applies only newer generations;
- `ParallelSyntaxService` plus a GPUI task that receives highlight results and installs them into the current `DiffViewer`;
- existing viewer/theme subscriptions.

Map actor phases to UI state without flicker:

- initial `Discovering`/`Loading` uses the loading panel;
- later `Loading` retains the current viewer;
- `Mutating` sets repository pending;
- an error with an existing snapshot stays in the viewer;
- a newer snapshot is installed through `DiffViewer::set_snapshot`;
- watcher warnings use the existing nonfatal repository-error surface.

Repository action handlers call `RepositoryClient`; if its bounded mutation queue is full or closed, show the typed error immediately. Scope controls send the latest desired scope rather than queueing every intermediate scope.

### 8. Current-terminal TUI integration

Replace `LocalBackend::apply` and direct watcher polling with the same `RepositoryService`. The terminal loop must never call `Handle::block_on(snapshot_with_sources)`.

- Seed the service with the already loaded initial `RepositorySnapshot` to avoid a redundant startup load.
- Submit `RepositoryAction`s to the client and map `RepositoryState` phase/error/snapshot changes into `DiffReviewState`.
- Drain repository state and syntax result receivers around the existing 100 ms crossterm poll; mark the state dirty when a new generation or highlight result is accepted.
- Configure `DiffReviewState` with the native background syntax sink. Keep the existing synchronous default for direct `diff-ratatui` users.
- Preserve the current theme persistence and review submission behavior.

### 9. External TUI session integration

Make the parent session process the repository-service owner so repository state remains authoritative. Extend the internal Unix-socket protocol (not `clankerdiff-protocol`'s public review response) with a lightweight state poll:

```rust
SessionRequest::RepositoryState { after_generation: u64 }
SessionResponse::RepositoryState {
    generation: u64,
    phase: SessionRepositoryPhase,
    document: Option<DiffDocument>,
    error: Option<String>,
    watcher_warning: Option<String>,
}
```

The parent starts `RepositoryService` with its initial snapshot. Repository actions enqueue work and return `Accepted` promptly; the attached child polls state from a background backend worker and installs a document only when the generation advances. Move socket I/O off the TUI draw/input loop. The server must accept request connections concurrently (bounded worker threads or Tokio tasks) so a slow poll cannot prevent submit/cancel, while all Git state still flows through the single repository actor.

The parent, not the attached child, owns `RepositoryWatcher`; remove duplicate child-side watching. The attached UI owns only its local syntax service. Add bounded socket read/write timeouts and close workers when submit/cancel ends the session.

### 10. Shutdown and subprocess behavior

Return explicit service handles whose `shutdown` closes senders, drops watchers, stops dispatchers, and awaits actor/dispatcher tasks. `Drop` should provide a best-effort abort fallback for UI teardown paths that cannot await.

Ensure every Tokio Git command uses `kill_on_drop(true)`, including ordinary `run`/`run_read` commands as well as the existing `CatFileBatch`. Generation suppression remains necessary because Rayon jobs cannot be forcibly aborted after starting.

## Implementation Steps

1. **Add concurrency dependencies and feature boundaries.**
   - Add workspace entries for `futures` and `rayon = "1.12.0"`; regenerate `Cargo.lock`.
   - Add `futures`, `rayon`, and needed Tokio `sync` features to `diff-git`.
   - Add a `parallel` feature to `diff-syntax` that enables Rayon/channel support. Enable it only in native hosts (`clankerdiff` and `diff-gpui-desktop`), not the web build.
   - Add direct `diff-syntax` dependencies to native hosts that construct `ParallelSyntaxService`.

2. **Make complete source text clone-cheap for owned jobs.**
   - Add `SourceDocument::text_shared() -> Arc<str>` next to `text()` in `diff-core/src/content.rs`.
   - Test that it points to the same allocation and preserves exact UTF-8/source identity.

3. **Harden watcher and Git child lifecycle primitives.**
   - Split watcher change and fatal-error delivery in `diff-git/src/watch.rs`; retain `receiver`/`drain` compatibility only if useful, but implement them atop the reliable split state.
   - Add tests where a pending change fills its channel before a fatal watcher callback and assert the error is still returned once.
   - Build ordinary Git processes with `Command::spawn`, `kill_on_drop(true)`, and `wait_with_output` rather than `Command::output` if needed to guarantee cancellation cleanup.

4. **Parallelize safe snapshot stages.**
   - Refactor `capture_sources` into raw loading and deterministic assembly phases.
   - Run worktree reads through `buffer_unordered(MAX_CONCURRENT_SOURCE_READS)`; leave `CatFileBatch` serialized.
   - Apply binary/UTF-8 classification, `SourceDocument` creation, archive budget, and map insertion in original document/side order.
   - Convert `document_from_captured_sources` to a Rayon-backed async helper. Use indexed `into_par_iter().map(...).collect()` and bridge completion with a one-shot.
   - Keep both metadata stability passes and worktree fingerprint comparison unchanged around these stages.

5. **Implement the shared repository actor in `diff-git/src/service.rs`.**
   - Define `RepositoryGeneration(u64)`, `RepositoryPhase`, `RepositoryState`, `RepositoryServiceError`, `RepositoryClient`, and `RepositoryServiceHandle`.
   - Support startup from a repository path and optional seeded snapshot.
   - Implement capacity-one refresh signaling, latest-value scope changes, bounded FIFO mutations, and latest-state subscription.
   - Implement the active read generation state machine, stale-completion suppression, mutation priority/serialization, watcher lifecycle, error retention, and graceful shutdown.
   - Re-export host-facing types from `diff-git/src/lib.rs`.

6. **Add repository actor integration tests.**
   - Create `diff-git/tests/repository_service.rs` using real temporary Git repositories and an extended reusable repository test builder.
   - Cover refresh bursts, a write during an in-flight load, scope change during load, external Git metadata changes, mutation followed by watcher feedback, mutation queue full/closed errors, stale completion suppression, watcher startup/runtime failure, and shutdown.
   - Add a test-only processing hook or fake backend only where deterministic in-flight ordering cannot be produced with real files; use a reusable builder with barriers rather than one-off mocks.

7. **Refactor syntax work into owned jobs and installable results.**
   - Add `HighlightProjection`, `HighlightJob`, `HighlightOutput`, `HighlightResult`, `HighlightJobSink`, and typed submission errors.
   - Add cache lookup/pending/install methods to `SyntaxHighlighter` while keeping all existing synchronous methods and cache keys compatible.
   - Use `Highlighter::fork()` for worker construction so grammar stores are shared.
   - Verify empty/unknown languages, injections, multiline projection, UTF-8 ranges, theme revisions, stats, and FIFO eviction retain current behavior.

8. **Implement `ParallelSyntaxService` behind the native feature.**
   - Add `diff-syntax/src/parallel.rs` with bounded submission, in-flight limits, Rayon dispatch, result delivery, and shutdown.
   - Test duplicate requests, queue-full retry, parallel old/new document completion, out-of-order results, old-theme rejection, worker failure fallback, and service closure.
   - Add a focused Criterion benchmark or benchmark case comparing serial and parallel highlighting for several independent medium/large source documents; assert correctness in tests, not timing.

9. **Teach reusable renderers to request background highlights.**
   - Extend `DiffViewerOptions` and `DiffViewer` with `HighlightExecution`, result installation, and background miss handling; update `diff_view.rs` to render plain text while pending.
   - Extend `DiffReviewState`, `cell_highlights`, and Ratatui render paths similarly.
   - Preserve synchronous defaults for web, Markdown, diff previews, examples, and third-party embedders unless they explicitly provide a sink.
   - Update existing performance-contract tests so settled frames still produce no misses/cells, and add background-mode tests proving render calls do not parse source bytes inline.

10. **Migrate desktop to the shared services.**
    - Replace `repository_worker` imports/fields with `RepositoryService` subscriptions.
    - Add syntax result receiver handling and call a new `DiffViewer::install_highlight_result` on the GPUI thread.
    - Handle generation ordering, actor phases, queue errors, existing-snapshot error presentation, and shutdown.
    - Delete `crates/diff-gpui-desktop/src/repository_worker.rs` and remove its module declaration after equivalent service tests exist in `diff-git`.

11. **Migrate current-terminal TUI to the shared services.**
    - Remove `LocalBackend` blocking Git execution and direct `RepositoryWatcher` ownership from `tui.rs`.
    - Introduce a small host/controller struct (in a new `review_worker.rs` if needed) that owns repository and syntax service handles and exposes nonblocking drains to the terminal loop.
    - Apply only newer generations through `DiffReviewState::set_snapshot`; route queue errors through `set_repository_error`.
    - Add tests with fake service channels for input responsiveness, state coalescing, stale generation rejection, syntax completion redraw, and shutdown.

12. **Move external sessions to parent-owned processing and background polling.**
    - Extend `protocol.rs` with the generation-aware repository-state request/response and size-limit tests.
    - Refactor `session.rs` to own `RepositoryService`, return actions promptly, serve bounded concurrent connections, and stop on submit/cancel.
    - Refactor `SessionBackend` so socket requests/polls run outside the terminal event loop; remove the attached process's watcher.
    - Test action/poll ordering, no duplicate watcher ownership, stale state suppression, poll timeout, concurrent cancel during processing, and protocol closure.

13. **Document and validate the architecture.**
    - Add `docs/architecture/processing.md` explaining ownership, state transitions, generation semantics, queue limits, I/O versus CPU pools, consistency boundaries, fallback paths, and shutdown.
    - Update README live-refresh wording to mention background/coalesced processing rather than implementation details.
    - Run formatting, checks, clippy, all features, native integration tests, renderer performance contracts, benchmark compilation, WASM checks, docs, and manual rapid-edit tests.

## Testing Plan

### Unit tests

- `RepositoryWatcher`:
  - mutation event filtering remains correct;
  - changed notifications coalesce;
  - fatal error delivery cannot be displaced by a pending change;
  - startup errors remain typed.
- Repository actor scheduler/state machine:
  - generations increase monotonically;
  - stale completion is never published;
  - repeated refreshes and scope changes coalesce to latest intent;
  - mutations retain FIFO ordering and cannot be silently dropped;
  - mutation completion forces one post-mutation snapshot;
  - errors retain the prior snapshot;
  - close/shutdown rejects new commands.
- Snapshot processing:
  - parallel derived documents equal serial fixtures for added/deleted/modified/renamed/copied/binary/oversized files;
  - indexed collection preserves file order;
  - concurrent source completion order does not change archive-budget outcomes;
  - unstable snapshots still retry/fail as before.
- Syntax jobs/service:
  - sync and async execution return identical spans;
  - duplicate keys create one job;
  - old theme/content results do not affect the active view;
  - queue-full removes pending status so later frames retry;
  - UTF-8, multiline/injection, unknown language, and empty source behavior remain correct;
  - shutdown does not leak workers or results.
- Renderer adapters:
  - synchronous mode preserves current tests;
  - background misses submit but perform zero parser bytes on the render thread;
  - installed results cause one redraw and then become cache hits.

### Integration tests

- Extend real temporary-repository tests to edit, create, rename, delete, stage, unstage, and commit while the service runs.
- Force an edit/scope change while a large snapshot is active and assert only the newest generation becomes visible.
- Queue a mutation during a read and verify mutation ordering plus the final snapshot.
- Verify desktop state mapping with GPUI's test harness where possible; otherwise keep actor behavior in `diff-git` and adapter mapping as pure tests.
- Verify current-terminal TUI accepts navigation/cancel input while a controlled repository load is pending.
- Verify external TUI polling receives parent watcher changes without a child watcher and can cancel while processing is active.
- Retain `diff-ratatui/tests/performance_contract.rs` and GPUI parse-once tests; add native background variants.
- Compile `diff-syntax` and `diff-ratatui` feature combinations and the GPUI web WASM target without native parallel features.

### Edge cases and manual verification

- Continuous editor writes faster than snapshot duration: memory stays bounded and the final settled contents eventually publish.
- Atomic-save rename sequences and external `git add/reset/commit`: one eventual authoritative refresh.
- Mutation-triggered watcher feedback: no infinite refresh loop and no pre-mutation snapshot published afterward.
- A repository becomes unstable for both snapshot attempts: previous view remains, error is shown, and a later refresh recovers.
- Syntax theme changes while old jobs run: old results are harmless and new-theme jobs render.
- A source hits the 8 MiB limit or the archive hits 64 MiB: limits and deterministic degradation remain unchanged.
- Single-core machine: Rayon degrades correctly without deadlock or worse semantics.
- inotify/resource exhaustion and watcher runtime failure: one warning, manual refresh still works.
- Closing desktop/TUI during Git, source-read, or syntax work: process exits and child Git processes are killed.

## Files to Modify/Create

| Path | Specific changes | Status |
|---|---|---|
| `Cargo.toml` | Add workspace-pinned `futures` and `rayon`. | Modify |
| `Cargo.lock` | Record direct concurrency dependencies/features. | Modify |
| `crates/diff-core/src/content.rs` | Add clone-cheap shared source text access for owned highlight jobs. | Modify |
| `crates/diff-git/Cargo.toml` | Add futures/Rayon and Tokio sync requirements. | Modify |
| `crates/diff-git/src/command.rs` | Ensure ordinary Git children are killed when cancelled/dropped. | Modify |
| `crates/diff-git/src/watch.rs` | Separate coalesced changes from reliable fatal-error delivery. | Modify |
| `crates/diff-git/src/repository.rs` | Add bounded concurrent source reads and Rayon per-file diff derivation while preserving stability/order/budgets. | Modify |
| `crates/diff-git/src/service.rs` | Implement the reusable generation-aware repository actor/client/state subscription. | Create |
| `crates/diff-git/src/lib.rs` | Export repository service types and revised watcher API. | Modify |
| `crates/diff-git/tests/repository_watch.rs` | Add fatal-error-under-backpressure and revised API contracts. | Modify |
| `crates/diff-git/tests/repository_service.rs` | Add real-repository actor/generation/backpressure/shutdown contracts. | Create |
| `crates/diff-syntax/Cargo.toml` | Add the native `parallel` feature and optional service dependencies/benchmark configuration. | Modify |
| `crates/diff-syntax/src/highlight.rs` | Split cache lookup/job creation/execution/result installation while preserving sync APIs. | Modify |
| `crates/diff-syntax/src/parallel.rs` | Implement bounded Rayon syntax dispatch using forked Arborium workers. | Create |
| `crates/diff-syntax/src/lib.rs` | Export job/result/sink APIs and feature-gated parallel service types. | Modify |
| `crates/diff-syntax/tests/api.rs` | Preserve and extend synchronous cache/parser contracts. | Modify |
| `crates/diff-syntax/tests/parallel.rs` | Test bounded async dispatch, deduplication, stale results, and shutdown. | Create |
| `crates/diff-syntax/benches/parallel.rs` | Benchmark independent serial versus parallel document highlighting. | Create |
| `crates/diff-gpui/src/viewer.rs` | Add background highlight mode, job submission, result installation, and redraw behavior. | Modify |
| `crates/diff-gpui/src/diff_view.rs` | Render plain text for pending background highlights rather than parsing inline. | Modify |
| `crates/diff-gpui-desktop/Cargo.toml` | Depend directly on native parallel syntax APIs; remove obsolete channel dependencies if unused. | Modify |
| `crates/diff-gpui-desktop/src/app.rs` | Own shared repository/syntax services, subscribe to latest state, enforce generations, and shut down cleanly. | Modify |
| `crates/diff-gpui-desktop/src/lib.rs` | Remove the desktop-only worker module and wire service lifecycle if needed. | Modify |
| `crates/diff-gpui-desktop/src/repository_worker.rs` | Remove after migration to `diff-git::RepositoryService`. | Remove |
| `crates/diff-ratatui/src/state.rs` | Configure background syntax execution and install completed results. | Modify |
| `crates/diff-ratatui/src/diff_preview.rs` | Return cached/plain spans and submit owned jobs on background misses. | Modify |
| `crates/diff-ratatui/src/render.rs` | Adapt rendering context to the background highlight lookup result. | Modify |
| `crates/diff-ratatui/tests/performance_contract.rs` | Add background-mode render-thread and settled-frame contracts. | Modify |
| `crates/clankerdiff/Cargo.toml` | Enable native parallel syntax support and required Tokio sync/runtime features. | Modify |
| `crates/clankerdiff/src/main.rs` | Seed/pass repository services into TUI/session flows rather than synchronous backends. | Modify |
| `crates/clankerdiff/src/tui.rs` | Remove blocking repository work/direct watching; drain repository and syntax updates nonblockingly. | Modify |
| `crates/clankerdiff/src/review_worker.rs` | Own common current/attached TUI service clients and update drains if separation keeps `tui.rs` focused. | Create |
| `crates/clankerdiff/src/protocol.rs` | Add generation-aware repository state polling and protocol tests. | Modify |
| `crates/clankerdiff/src/session.rs` | Own the parent repository actor, serve bounded concurrent requests, and support prompt action/state responses. | Modify |
| `README.md` | Update live-refresh/background-processing behavior and fallback wording. | Modify |
| `docs/architecture/processing.md` | Document actor ownership, generations, pools, bounds, consistency, and shutdown. | Create |

## Additional Notes

- This plan deliberately **does not** derive partial Git snapshots from notify event paths. Full snapshots plus generation suppression provide a simpler correct baseline across worktree/index changes, renames, untracked files, linked operations, and missed/coalesced events.
- This plan also does not move `ReviewSession` reconciliation or `DiffPresentation` rebuilding off the UI thread. Those operations are stateful and relatively entangled with selection/projection. Profile them after Git diffing and Tree-sitter parsing are removed from UI threads; if they remain material, follow up by introducing a revisioned prepared-presentation API rather than replacing a cloned session and losing user input.
- Tokio's official guidance warns that unconstrained `spawn_blocking` is inappropriate for large CPU fan-out and cannot cancel started work; Rayon is therefore the dedicated CPU pool, while generations supply correctness for late completions.
- Tokio's channel guidance recommends bounded queues and explicit backpressure. The only coalescing/lossy channels here represent latest intent (`refresh`, `scope`, latest published state); user mutations are bounded and reject overflow visibly.
- Arborium explicitly documents `Highlighter::fork()` as the thread-safe parallel-highlighting mechanism because forks share a grammar store and own independent parse contexts.
- Salsa remains a possible future step once processing can be expressed as stable pure per-file queries. The job/cache-key APIs in this plan avoid preventing that migration without paying Salsa's integration cost now.
- Watcher support on NFS, WSL mounts, containers, and linked worktree metadata remains subject to platform behavior. Manual refresh remains the fallback; a polling watcher can be added later without changing the repository actor contract.
- Recommended implementation sequence is steps 1–6 (repository safety), 7–9 (syntax primitives/adapters), then 10–12 (host migrations). Keep commits split along these boundaries so correctness and performance changes can be reviewed independently.