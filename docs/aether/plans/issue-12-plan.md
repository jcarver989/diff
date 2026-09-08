# Plan — Issue #12: Expose git diff mode switching in desktop, web, and TUI

## Overview

### Problem statement
The underlying crates already model git diff mode as `DiffScope` (`Unstaged` / `Staged` / `Both`, default `Both`) with full runtime support (`GitRepository::diff_args` mapping, `RepositoryWatcher::SetScope` live re-snapshot, CLI `--scope` initial selection), but the three user surfaces do not expose a clear, discoverable way to switch it:

- **Desktop** (`crates/diff-gpui-desktop/src/app.rs`): only wires scope via a hidden `CycleScope` action (`cmd/ctrl-shift-v`, `DesktopDiff` context). The shortcut is not in the shared `DiffViewer` `?` help (it is host-level), has no menu entry (`menus.rs` only has About/Edit/Window), has no clickable control in `Ready` state, and is only advertised in the `LoadState::Empty` detail string (`"Scope: {} · press ⇧⌘/Ctrl+V to change scope"`).
- **TUI** (`crates/diff-ratatui` widget + `crates/clankerdiff/src/tui.rs` host + `crates/clankerdiff/src/protocol.rs` + `session.rs`): freezes scope at launch. `DiffReviewEvent` (`crates/diff-core/src/review.rs:290`) has no scope variant, `DiffReviewState`/`ReviewSession`/`DiffDocument` carry no scope, there is no keybinding (`v` cycles `ViewMode`, `s` submits, `S` is free), and the Unix-socket session protocol (`SessionRequest`/`SessionResponse`) never carries scope, so an attached (`attach`) client can neither see nor change scope.
- **Web** (`crates/diff-gpui-web/src/lib.rs`): host-driven document-push model. The push envelope (`DocumentPayload`: bare `DiffDocument` or `{revision, request_id, document}`) has no `scope`, there is no request event back to the host for scope changes, and the canvas shows no scope label or control (startup shows `demo_document()` with no scope indication).

This makes it hard for users to see staged vs unstaged changes without restarting with a different `--scope` flag.

### Success criteria / acceptance conditions
- Each UI shows the current scope persistently (not only in the empty state) and offers a visible, clickable/keyboard affordance to change it:
  - TUI: scope pill in the frame/header + `S` (Shift-S) keybinding listed in footer hint and `?` help; works in both `run_local` and `attach` paths.
  - Desktop: persistent segmented control (Unstaged / Staged / Both) visible in both `Empty` and `Ready` states + retained `CycleScope` shortcut + a `View > Scope` menu entry; shortcut documented where the empty-state text already advertises it.
  - Web: scope label + clickable segmented control on the canvas; clicking dispatches a host request event; pushed documents can carry scope and update the label; acknowledged via existing `diff-review-document-applied` flow.
- Renderers stay git-free: both `DiffViewer` (gpui) and `DiffReviewWidget` (ratatui) emit an intent; only hosts (`DesktopApp`, TUI `LocalBackend`/`SessionBackend`+`session.rs` server, web host JS) execute the scope change via `RepositoryWatcher::SetScope` or host-owned refresh.
- No regressions: existing scope semantics unchanged (`diff_args` mapping, `DiffScope::next` cycle order `Unstaged → Staged → Both → Unstaged`, `--scope` default `both`, protocol stdout envelope `ReviewResponse::Diff.scope` echo).
- Tests per repo conventions (integration tests in `tests/`, real objects/fakes, test-builder pattern, `Result`-returning tests with `?` instead of `unwrap`): core event round-trip, TUI keybinding + empty-state pill, session protocol scope switch, desktop scope command, web envelope + request event + spec update.

## Technical Approach

### High-level architectural decisions
1. **One shared intent, three hosts.** Add `DiffReviewEvent::SetScope(DiffScope)` in `diff-core` (re-exported as `DiffViewerEvent` by `diff-gpui` and aliased as `DiffReviewEvent` by `diff-ratatui`). This mirrors the existing `RepositoryAction` pattern documented on `RepositoryAction` ("Renderers emit intents but never execute Git themselves"). Both renderers map their scope controls to this single variant; each host translates it the way it already translates `RepositoryAction`:
   - Desktop `DesktopApp::handle_viewer_event` → existing `HostCommand::SetScope` → `RepositoryRequest::SetScope`.
   - TUI local path → `watcher.request_tx.send(RepositoryRequest::SetScope)` (same channel desktop uses).
   - TUI attach path → new `SessionRequest::SetScope` over the socket, handled by `session.rs` server via the same `request_tx`.
   - Web `WebRoot` viewer subscription → new `diff-review-set-scope` `CustomEvent` with `{"scope": "<unstaged|staged|both>"}` back to the host (mirrors `diff-review-repository-action`); inbound scope arrives via the extended `diff-review-set-document` envelope.
2. **Carry scope alongside documents, not inside `DiffDocument`.** `DiffDocument` deliberately carries only `repo_root + files` (scope lives on `RepositorySnapshot.scope` and parse-time `StageState` fallback). Do not add scope to `DiffDocument`; instead thread an explicit scope field through each transport that already parallels snapshots:
   - Session socket: add `scope` to `SessionResponse::Document` (and expose current scope on `Unchanged` so a poll can reconcile the label without new content), plus a `SessionRequest::SetScope` variant with `Accepted`/`RepositoryError` replies reusing the existing single-pending gate.
   - Web envelope: add optional `scope` to `DocumentPayload::Envelope` and `DocumentCommand` (`Option<DiffScope>`, string-serialized via `FromStr`/`as_str`); bare pushes mean "scope unknown/unchanged" and preserve the current label.
   - TUI `HostState`/`HostUpdates` watch channel and `DiffReviewState`: add a `scope: DiffScope` field with `set_scope`/`scope()` accessors so the widget can render the pill even before the next document arrives.
3. **Visible pill/segmented controls using existing components only.** No new design system:
   - GPUI (desktop + web share `DiffViewer`): render a scope segment in the existing chrome (desktop host header above the viewer; web inside `WebRoot::render` or `DiffViewer::render_review_bar` extension point) built from `ui::prelude::Button` with `selected()` state (already supports `selected: bool`, `ControlState{selected}`, `aria_label`), matching `review_bar.rs` button patterns (`Button::new(id, label, theme).size(Small).selected(...).on_click(...)`).
   - Ratatui: render a `[Scope: both]` pill in the existing `AppFrame` title area (`render.rs` `DiffReviewWidget::title` / `render_body` empty-state branch) and extend the one-line footer hint + `render_help` modal text; reuse `ActionBar`/`EmptyState`/`NoticeTone` already in use.
4. **Keyboard: fill the free slot per surface, keep back-compat.**
   - TUI: `S` (Shift-S, `KeyCode::Char('S')`) cycles scope in browse mode (both panes). `s` (submit), `v` (view-mode), `a/A` (stage-all), `C`/`d` are taken; `S` is currently unbound in `handle_browse_key`. Respect the existing `RepositoryOperationStatus::Pending` gate pattern (ignore or defer while pending, consistent with hosts allowing one command at a time).
   - Desktop: keep `cmd/ctrl-shift-v → CycleScope` bindings (do not break muscle memory); add explicit `SetScope(Unstaged|Staged|Both)` menu actions under a new `View` menu so each scope is directly reachable by menu; segmented buttons call the same `HostCommand::SetScope`.
   - Web: clicks only (browser host owns focus/keyboard); no new viewer keybinding, avoiding collision with the viewer's `DiffViewer::bind_keys` map.

### Design patterns to employ
- Intent/event pattern already in the codebase (`DiffReviewEvent` → host `command()` / `handle_viewer_event` / `apply_event`).
- Single-pending-command gate already used in all three hosts (desktop `command_task`, TUI `commands` channel + `repository_pending`, web `RepositoryRequests::begin/finish`): scope commands reuse the same gate and `set_repository_pending(true/false)` + `set_repository_error` surfaces.
- Test-builder pattern for fixtures (`RepoFixtureBuilder`, `DiffDocument::empty()`, `demo_document()`), in-memory fakes over mocks (`FakeFileWatcher` extension if needed; prefer real `RepositoryWatcher` + temp git repos as `session.rs` tests already do).
- Envelope tolerance: web `DocumentPayload` is `#[serde(untagged)]` and protocol crate accepts unknown fields; add `scope` as `Option<String>`-parsed-via-`FromStr` with `#[serde(default)]` so old hosts pushing without scope keep working.

### Key technical considerations and trade-offs
- **New `DiffReviewEvent` variant is a breaking serde shape change** (externally-tagged enum gains `SetScope`). Chosen over a parallel channel because both renderers (`diff-gpui`, `diff-ratatui`) already funnel every user intent through this one enum and every host already matches on it exhaustively; the compiler will surface all match sites (`app.rs handle_viewer_event` + `host_event_effect`, `tui.rs apply_event`, web `dispatch_viewer_event` + subscription). Alternative of a separate `on_scope_change` callback was rejected: it would fork the event plumbing in three hosts and two test harnesses (`diff-gpui/testing.rs`, `diff-ratatui/testing.rs`).
- **Session protocol versioning.** `SessionRequest`/`SessionResponse` have no version field (length-delimited JSON, `MAX_REQUEST_BYTES` 8MB / `MAX_RESPONSE_BYTES` 128MB). Adding `SetScope` request and `scope` response fields is wire-incompatible with a stale counterpart, but server and client are always the same binary (`attach_command` re-invokes `current_exe`), so lockstep upgrade is safe. Keep `ProtocolError` handling unchanged; unknown-variant JSON from a newer peer surfaces as `Json` error, same as today.
- **Scope label vs snapshot truth.** After a scope command, the authoritative scope arrives with the next snapshot (`snapshot.scope`, adopted in `apply_watch_state` / `HostUpdates::drain`). Renderers should optimistically show the requested scope as pending (via `repository_pending`) but adopt the snapshot's scope on arrival, avoiding divergence when the watcher reports an error.
- **Empty-state behavior.** Scope switching must work from empty states (the exact pain point: "No changes" with no way to look at the other mode). Desktop root `div().key_context("DesktopDiff").on_action(cycle_scope)` already wraps all `LoadState`s; keep that and add the segmented control to `status_panel`/`Empty` branch. TUI `render_body` empty branch (`"No changes"`) must also show the pill and still accept `S`. Web with empty `files: []` must still render the scope control.
- **No `DiffDocument` change.** Avoids touching parser, fingerprint, syntax, markdown, and the large `demo-document.json` fixture. Scope travels out-of-band in each transport.
- **Docs:** update CLI `--help` text only if wording changes; the main doc work is the `?` / help-modal rows and the web `tests/web.spec.ts` protocol documentation-by-test plus any `README` mention of scope switching if present.

## Implementation Steps

### Step 1 — Core: add the shared `SetScope` intent
- File: `crates/diff-core/src/review.rs`
  - Add `DiffScope` to the imports from `crate` (or `super`) and add variant `SetScope(DiffScope)` to `DiffReviewEvent` with a doc line matching the enum's intent style.
  - Verify exhaustive matches still compile (they must be updated in Steps 3–5; this step alone will fail `cargo check` intentionally until hosts are updated — land Steps 1–5 atomically or gate with `#[non_exhaustive]`-free full updates in one PR).
- Pseudo-code:
  ```rust
  pub enum DiffReviewEvent {
      RepositoryAction(RepositoryAction),
      SetScope(DiffScope),
      SubmitReview(ReviewSubmission),
      CopyFormattedReview(String),
      Cancel,
  }
  ```
- Acceptance: `cargo check -p clankerdiff-core` passes; `review.rs` round-trip test pattern extended (see Testing Plan).

### Step 2 — TUI renderer (`clankerdiff-ratatui`): state + keybinding + visible pill
- Files: `crates/diff-ratatui/src/state.rs`, `src/input.rs`, `src/render.rs`, `src/lib.rs` (if re-export list changes)
  - `state.rs`: add `scope: DiffScope` field (default `Both`) to `DiffReviewState`; add `pub fn scope(&self) -> DiffScope`, `pub fn set_scope(&mut self, scope: DiffScope)` (marks dirty), and extend `set_document`/`new`/`with_theme` signatures only if needed to accept initial scope (prefer `new(document)` unchanged + `set_scope` call by host to avoid breaking `examples/review.rs` and `tests/support`).
  - `input.rs` `handle_browse_key`: add `KeyCode::Char('S') => return Some(DiffReviewEvent::SetScope(self.scope.next()))` (cycle semantics match `DiffScope::next`). Place after the `v` (CycleViewMode) arm. Honor the pending gate at the top of the function (extend the `matches!(key.code, ...)` guard to include `'S'` alongside `' ' | 'a' | 'A' | 'C' | 'd' | 'r'` if scope commands must be blocked while pending; otherwise allow and let host gate drop it — prefer blocking for consistency).
  - `render.rs`: (a) `render_body` empty branch: change `"No changes"` to include scope, e.g. `format!("No changes (scope: {})", state.scope())` or a `[Scope: {} · S to change]` pill line; (b) `render_footer` hint strings: append `[S] scope` to the Files/Diff hint variants; (c) `render_help` modal text: add `S  cycle scope (unstaged/staged/both)` under the Git section; (d) optional header: prefix `DiffReviewWidget::title` default `"Diff Review"` with scope, e.g. `"Diff Review · both"`, only if it does not break `tests/widget.rs` golden snapshots (otherwise keep title and rely on empty-state + footer + help).
- Acceptance: pressing `S` in the widget test harness yields `DiffReviewEvent::SetScope(_)`; empty document renders a scope indicator.

### Step 3 — TUI host + session protocol (`clankerdiff` binary crate)
- Files: `crates/clankerdiff/src/protocol.rs`, `src/session.rs`, `src/tui.rs`
  - `protocol.rs`: add `SessionRequest::SetScope { scope: DiffScope }` (or tuple `SetScope(DiffScope)` matching enum style) + `SessionRequestRef::SetScope` borrow variant; add `scope: DiffScope` (or `Option<DiffScope>` for back-compat — prefer required `DiffScope` since both ends upgrade together; document choice in code) to `SessionResponse::Document` and `SessionResponseRef::Document`; optionally add `scope` to `Unchanged` so attach clients can reconcile the label on polls with no content change. Extend the `exchanges_length_delimited_messages` test.
  - `session.rs` `handle_connection`: add `SessionRequest::SetScope { scope }` arm that sends `RepositoryRequest::SetScope { scope, result_tx }` over `watcher.request_tx` (clone the sender per connection, mirroring `diff-gpui-desktop/src/app.rs:207-220`), awaits the oneshot, and replies `Accepted` or `RepositoryError(String)`. Thread the watcher's current scope into `Document`/`Unchanged` responses (track `last_scope: DiffScope` alongside `snapshot`/`revision` in `run_blocking`; adopt `snapshot.scope` whenever the snapshot pointer changes).
  - `tui.rs`:
    - `HostState`: add `scope: DiffScope`; `HostUpdates::drain` must call `state.set_scope(latest.scope)` alongside `set_document`.
    - `run_local` bridge: propagate `snapshot.scope` into `HostState` when the watched snapshot changes (the `subscription.changed()` loop already compares `snapshot != installed`; copy `scope` there).
    - `SessionBackend` (attach path): on `Document`/`Unchanged` responses update `HostState.scope`; add `apply_scope(scope)` sending `SessionRequestRef::SetScope` via `request_at` (same fresh-`UnixStream`-per-request pattern, 30s timeouts) with the one-command-at-a-time pending gate.
    - `apply_event` (lines ~499-523): map `DiffReviewEvent::SetScope(scope)` to `backend.apply_scope(scope)` + `state.set_repository_pending()`-equivalent, mirroring the `RepositoryAction` path; `CopyFormattedReview` swallowing and `Cancel`/`SubmitReview` termination stay unchanged.
    - `attach()` initial state: `DiffReviewState::new(...)` then `set_scope(initial.scope)` from the first `Document` poll.
- Acceptance: local TUI `S` keypress re-snapshots with the next scope; attached TUI `S` performs a socket round-trip and the pill updates on the next poll.

### Step 4 — Desktop host (`clankerdiff-gpui-desktop` + shared `clankerdiff-gpui` help)
- Files: `crates/diff-gpui-desktop/src/app.rs`, `src/menus.rs`, `src/lib.rs` (only if menu registration changes), `crates/diff-gpui/src/shortcuts.rs` (help text) and optionally `src/review_bar.rs`
  - `app.rs`:
    - `handle_viewer_event`: add `DiffReviewEvent::SetScope(scope)` → `self.command(HostCommand::SetScope(scope), cx)` (same as `cycle_scope` but with explicit scope from the segmented control). Update `host_event_effect` match with the new variant → `HostEventEffect::None` (a scope request is not copy/print/quit).
    - Render: add a persistent scope segmented control visible in both `LoadState::Empty` and `LoadState::Ready`. Recommended placement: a thin toolbar row above the viewer/empty panel inside the root `div` (which already sets `key_context("DesktopDiff")`), built from three `Button::new("scope-unstaged"|"scope-staged"|"scope-both", label, theme).size(Small).selected(self.scope == ...)` elements whose `on_click` handlers call `this.command(HostCommand::SetScope(scope), cx)`; disable while `command_task.is_some()` via `.disabled(...)`. Keep the existing `Empty` detail text (it already advertises the shortcut) and append the same control below it.
    - Keep `cycle_scope` (`self.scope.next()`) and its `cmd/ctrl-shift-v` bindings untouched for back-compat.
  - `menus.rs`: add a `View` menu with three scope items (`Unstaged`/`Staged`/`Both`) bound to new `desktop_diff`-namespace or `desktop_menu`-namespace actions carrying a `DiffScope` payload (follow the `NoopEdit`/`Action` derive pattern already in the file). Register in `register_actions` + `build()`; extend `lib.rs` tests `desktop_menus_expose_app_edit_and_window_menus` accordingly.
  - `diff-gpui/src/shortcuts.rs`: the `?` modal lists viewer-level keys only; since `CycleScope` remains host-level, do not add it to the viewer's `GIT` table (which would falsely promise the binding inside bare `DiffViewer` embeds). Instead document the desktop shortcut in the desktop empty-state text (already there) and, if a desktop-level help affordance exists, there. If the team prefers the shortcut visible in `?`, the alternative is promoting scope actions into `viewer.rs actions!` + `bind_keys` — flag as a review decision (see Additional Notes).
- Acceptance: clicking a segment or using the menu/cycle shortcut issues exactly one `RepositoryRequest::SetScope`; `apply_watch_state` adopts `snapshot.scope`; pending state disables the control and shows via `set_repository_pending`.

### Step 5 — Web host (`clankerdiff-gpui-web`)
- File: `crates/diff-gpui-web/src/lib.rs`, `tests/web.spec.ts`, `demo-document.json` (only if a scope fixture field is added — prefer not to touch the 2.4MB fixture)
  - Protocol (non-wasm, testable on host target):
    - `DocumentPayload::Envelope`: add `#[serde(default)] scope: Option<String>`; `DocumentCommand`: add `pub scope: Option<DiffScope>` parsed via `DiffScope::from_str` (invalid string → `WebError::InvalidDocument`, same error path as malformed JSON). `decode_document_command` maps the string; `decode_document` unchanged.
    - `WebCommand::SetDocument`: add `scope: Option<DiffScope>`; `WebRoot::apply_command` updates a new `current_scope: Option<DiffScope>` field (initial `None` = "host has not said"; display falls back to no label rather than guessing) and calls `viewer.set_document` as today; `document_update_decision` logic unchanged (scope does not affect staleness).
    - New outbound event: define `pub const SCOPE_REQUEST_EVENT: &str = "diff-review-set-scope"`; `dispatch_scope_request(scope: DiffScope)` serializes `{"scope": scope.as_str()}` via `dispatch_custom_event` (same helper as `dispatch_repository_action`). In the `WebRoot` viewer subscription, map `DiffReviewEvent::SetScope(scope)` to this dispatch (the `RepositoryAction` arm stays as-is; `dispatch_viewer_event` match gains a `SetScope` arm that is unreachable from that path since the root intercepts it first — mirror the existing `RepositoryAction(_) => return` comment).
    - `WebRoot::render` / review-bar area: render the current scope label + three segmented `Button`s (reuse `ui::prelude::Button` + `selected()`) above or beside the viewer; clicks emit `DiffReviewEvent::SetScope` through the normal viewer-event subscription path (i.e., the control can live in `WebRoot` and directly call `dispatch_scope_request`, or live in `DiffViewer` and emit the event — prefer `WebRoot` so bare `DiffViewer` embeds stay scope-agnostic, consistent with desktop keeping `CycleScope` host-level).
    - Host listeners: no new inbound listener needed beyond the extended `diff-review-set-document` (host answers a scope request by pushing a document envelope carrying the new `scope` + refreshed content, optionally with the same `request_id` completion semantics already tested).
  - `tests/web.spec.ts`: extend the browser smoke test: push `{revision, scope: "staged", document}` and assert the canvas shows the scope label; click the `Both` segment and assert a `diff-review-set-scope` event with `{"scope":"both"}` is dispatched; push the answering envelope and assert `diff-review-document-applied` fires.
- Acceptance: scope label reflects the last pushed envelope; user clicks produce exactly one host event per click; stale-revision + single-pending semantics from the existing spec still hold.

### Step 6 — Wash-up: help text, CLI docs, changelog
- TUI `examples/review.rs`: document the `S` binding in its key list comment if it enumerates keys.
- Desktop `args.rs` `USAGE` text: unchanged (initial `--scope` already documented); verify.
- If the repo has a `README`/`docs` mention of diff modes, add one line per UI describing how to switch; otherwise skip docs per "never add comments/docs unless instructed" convention — code-adjacent help strings above are the documentation.
- Run `cargo fmt`, `cargo clippy --workspace -- -D warnings` (repo sets `pedantic`), and the affected test suites (see Testing Plan).

## Testing Plan

Follow repo testing conventions: integration tests in `tests/` over unit tests; test only public APIs; real objects (`RepoFixtureBuilder`, real `RepositoryWatcher`, real socket pairs) and existing fakes/builders extended rather than bespoke mocks; shared helpers at the bottom of files; `Result`-returning tests using `?`, no `unwrap`.

- **Unit-ish integration (public API, `tests/` dirs):**
  - `crates/diff-core/tests/`: extend scope/event coverage — `DiffReviewEvent::SetScope` serde round-trip for all three scopes plus `DiffScope::next` cycle order (build on existing `tests/diff_scope.rs` and `review.rs::repository_actions_round_trip` pattern).
  - `crates/clankerdiff` `src/protocol.rs` `#[cfg(test)]`: extend `exchanges_length_delimited_messages` to cover `SetScope` request and `Document`-with-scope response.
  - `crates/diff-gpui-web` host-target tests (`src/lib.rs #[cfg(test)]`, lines ~696-857): `decode_document_command` accepts bare doc, envelope without scope (→ `None`), envelope with each valid scope string, and rejects invalid scope; `document_update_decision` unaffected by scope.
- **Integration tests required:**
  - `crates/diff-watch/tests/repository_watcher_tests.rs` (already covers `SetScope` ack/republication): add an assertion that the republished `RepositorySnapshot.scope` equals the requested scope for a staged-vs-unstaged fixture (use `RepoFixtureBuilder` staged/unstaged file states already in the file).
  - `crates/clankerdiff/src/session.rs` `#[cfg(test)]` (`a_polling_client_only_receives_changed_documents` pattern): new test `scope_switch_republishes_with_new_scope` — spawn watcher on a fixture with both staged and unstaged changes, `run` the session on a thread, send `SetScope(Staged)` via `SessionRequestRef`, expect `Accepted`, then poll `Document{revision}` until a `Document` with `scope == Staged` and only-staged content arrives.
  - `crates/diff-ratatui/tests/widget.rs` (test-support harness): new test pressing `S` (`KeyCode::Char('S')`) on a non-empty document yields `Some(DiffReviewEvent::SetScope(DiffScope::Staged))` from `Both`-initial state, and on an empty document the rendered buffer contains the scope pill text.
  - `crates/diff-gpui/tests/keyboard.rs` / `viewer.rs` (GPUI test harness): test that activating the new scope segmented control (or its action, depending on Step 4 placement) emits `DiffViewerEvent::SetScope` on a real window; extend `design_system.rs` only if the segmented control becomes a shared component.
  - `crates/diff-gpui-desktop` (`src/app.rs #[cfg(test)]` + `src/lib.rs` menu test): extend `desktop_menus_expose_app_edit_and_window_menus` to assert the new `View` menu and its three scope actions.
  - `crates/diff-gpui-web/tests/web.spec.ts` (vitest + Playwright chromium, `npm test = trunk build && vitest run --browser`): extend as described in Step 5 (scope label assertion, `diff-review-set-scope` request assertion, answering envelope, no-regression on stale/single-pending assertions).
  - `crates/clankerdiff/tests/cli_protocol.rs` + `clankerdiff-protocol/tests/fixtures.rs`: no change expected (final `ReviewResponse::Diff.scope` echo already exists); add a case asserting the echoed scope equals the scope active at submission if the session test above makes it trivial.
- **Edge cases to verify:**
  - Scope switch while a repository command is pending → second command dropped (single-pending gate) with no deadlock; pending indicator shown.
  - Scope switch on an empty result set → `Empty` state with correct scope label; switching back restores content without restart.
  - Corrupt-index background error during a scope switch → `RepositoryError` reply, error surface via `set_repository_error`, last-good document retained (existing `exercise_health` pattern).
  - Web: envelope without `scope` preserves the current label; stale `revision` with a scope field does not move the label (`SkipStale` path); bare push after scoped pushes preserves the label.
  - Attach client started before a scope change sees the new scope on its next `Document` poll (`Unchanged`-carries-scope or revision bump — assert whichever Step 3 implements).
  - `Both` on a repo with no `HEAD` still uses `EMPTY_TREE` (existing `diff-git` contract, unchanged).

## Files to Modify/Create

| Path | Changes | Add / Modify / Remove |
|---|---|---|
| `docs/aether/plans/issue-12-plan.md` | This plan file | Add |
| `crates/diff-core/src/review.rs` | Add `DiffReviewEvent::SetScope(DiffScope)` variant + import; extend tests | Modify |
| `crates/diff-ratatui/src/state.rs` | Add `scope` field + `scope()`/`set_scope()` accessors | Modify |
| `crates/diff-ratatui/src/input.rs` | Bind `S` to `SetScope(self.scope.next())` + pending gate + help-adjacent key handling | Modify |
| `crates/diff-ratatui/src/render.rs` | Scope pill in title/empty-state, footer hint, `render_help` Git row | Modify |
| `crates/diff-ratatui/tests/widget.rs` | `S`-key intent test + empty-state pill render assertion | Modify |
| `crates/clankerdiff/src/protocol.rs` | `SessionRequest::SetScope` + `scope` on `SessionResponse::Document`/`Unchanged` + test | Modify |
| `crates/clankerdiff/src/session.rs` | Handle `SetScope` via `watcher.request_tx`; track/publish current scope; new session test | Modify |
| `crates/clankerdiff/src/tui.rs` | `HostState.scope`, bridge/`SessionBackend` scope propagation, `apply_event` `SetScope` arm, `attach()` init | Modify |
| `crates/diff-gpui-desktop/src/app.rs` | `SetScope` event handling, persistent segmented scope control (Empty + Ready), keep `CycleScope` | Modify |
| `crates/diff-gpui-desktop/src/menus.rs` | New `View > Scope` menu (Unstaged/Staged/Both actions) + registration | Modify |
| `crates/diff-gpui-desktop/src/lib.rs` | Extend menu test for the `View` menu (no runtime change) | Modify |
| `crates/diff-gpui/src/shortcuts.rs` | Only if team opts into viewer-level scope help row (default: no change; decision logged in review) | Modify |
| `crates/diff-gpui/tests/keyboard.rs` or `viewer.rs` | Scope control emits `DiffViewerEvent::SetScope` on a real window | Modify |
| `crates/diff-gpui-web/src/lib.rs` | Optional `scope` in `DocumentPayload`/`DocumentCommand`/`WebCommand`, `current_scope` on `WebRoot`, `diff-review-set-scope` dispatch, scope label + segmented control | Modify |
| `crates/diff-gpui-web/tests/web.spec.ts` | Scope label + request-event + answering-envelope assertions | Modify |
| `crates/diff-core/tests/diff_scope.rs` (or `review` tests) | `SetScope` serde round-trip + `next()` order | Modify |
| `crates/diff-watch/tests/repository_watcher_tests.rs` | Assert republished `snapshot.scope` follows `SetScope` | Modify |
| `crates/diff-ratatui/examples/review.rs` | Mention `S` binding if it lists keys | Modify |
| `crates/diff-git/src/repository.rs` | No change (semantics already correct; `diff_args`/`content_location` untouched) | — |
| `crates/clankerdiff-protocol/src/lib.rs` | No change (final `ReviewResponse::Diff.scope` echo already correct) | — |

## Additional Notes

- **Decision for reviewer:** whether `CycleScope` (and the new explicit scope actions) should be promoted from the desktop host into shared `DiffViewer` (`viewer.rs actions!` + `bind_keys` + `?` modal). Recommendation in this plan is to keep scope host-level (renderers emit the intent; hosts own keybindings/menus) because the web host must implement scope as host-driven pushes anyway and bare `DiffViewer` embeds have no watcher to command. If promoted, `shortcuts.rs` `GIT` table and `viewer.rs` key contexts gain scope rows, and the TUI/desktop/web key help must be reconciled.
- **Alternative considered and rejected:** putting `scope` inside `DiffDocument`. It would force parser, fingerprint, snapshot-caching, and fixture churn for a display concern; out-of-band transport fields achieve the same UX with a fraction of the blast radius.
- **Follow-up tasks that may be spawned:** (1) persist last-used scope in `preferences.rs` (desktop) / `localStorage` (web, alongside `clankerdiff.theme.v1`) / TUI preferences; (2) per-file scope overrides or a `HEAD..branch` range mode (requires `DiffScope` extension + `diff_args` + both CLI parsers + protocol strings); (3) agent-protocol documentation for the new `diff-review-set-scope` web event and `SetScope` socket message.
- **Documentation updates needed:** in-code help only (TUI footer + `?` modal, desktop empty-state text already exists, web event name const docs). No new markdown docs unless the maintainers request them, per repo "never add comments/docs unless instructed" convention (help strings are UI copy, not code comments, and are therefore in scope).
- **Risk callout:** `cargo clippy --workspace` runs with `pedantic`; new `match` arms on `DiffReviewEvent` must not leave wildcard arms that the lint flags, and new GPUI `Action` structs must derive `Deserialize`/`JsonSchema` like `NoopEdit` does.
