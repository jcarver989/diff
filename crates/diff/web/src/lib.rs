pub mod commands;

use clankerdiff_core::{DiffDocument, DiffScope};
use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ui_core::ReviewTheme;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub const REFRESH_REQUEST_EVENT: &str = "diff-review-refresh";

/// Errors returned while validating commands from JavaScript.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// The supplied document was not valid `DiffDocument` JSON.
    #[error("invalid diff document JSON: {0}")]
    InvalidDocument(#[from] serde_json::Error),
    /// The supplied Markdown source payload was malformed.
    #[error("invalid Markdown document payload: {0}")]
    InvalidMarkdownPayload(serde_json::Error),
    /// The selected embedded theme is not available.
    #[error("unknown built-in theme `{0}`")]
    UnknownTheme(String),
    /// The supplied scope string was not a valid `DiffScope`.
    #[error("invalid diff scope `{0}`")]
    InvalidScope(String),
    /// The viewer command channel has not been installed yet.
    #[error("the diff viewer has not started")]
    NotStarted,
    /// The viewer command channel is no longer accepting commands.
    #[error("the diff viewer command channel is closed")]
    CommandChannelClosed,
}

/// A pushed document and its optional host revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCommand {
    /// Monotonic host revision, when the host supplies one.
    pub revision: Option<u64>,
    /// The command completed by this push, if any.
    pub request_id: Option<u64>,
    pub scope: Option<DiffScope>,
    pub document: DiffDocument,
}

/// Wire form of a pushed document: a bare document or a revision envelope.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DocumentPayload {
    Envelope {
        #[serde(default)]
        revision: Option<u64>,
        #[serde(default)]
        request_id: Option<u64>,
        #[serde(default)]
        scope: Option<String>,
        document: DiffDocument,
    },
    Bare(DiffDocument),
}

/// Decodes a pushed document, accepting a bare document or a revision envelope.
///
/// # Errors
/// Returns an error when `json` is neither form.
pub fn decode_document_command(json: &str) -> Result<DocumentCommand, WebError> {
    match serde_json::from_str::<DocumentPayload>(json)? {
        DocumentPayload::Envelope { revision, request_id, scope, document } => {
            let scope = scope
                .map(|value| DiffScope::from_str(&value).map_err(|_| WebError::InvalidScope(value)))
                .transpose()?;
            Ok(DocumentCommand { revision, request_id, scope, document })
        }
        DocumentPayload::Bare(document) => {
            Ok(DocumentCommand { revision: None, request_id: None, scope: None, document })
        }
    }
}

pub const TARGET_REQUEST_EVENT: &str = "diff-review-set-target";

/// Decodes a serialized diff document command.
///
/// # Errors
/// Returns an error when `json` is not a valid diff document.
pub fn decode_document(json: &str) -> Result<DiffDocument, WebError> {
    decode_document_command(json).map(|command| command.document)
}

/// What the shell should do with one pushed document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDecision {
    /// Install the document and acknowledge a change.
    Apply,
    /// The host revision is not newer than the applied one.
    SkipStale,
    /// The document is identical to the one already installed.
    SkipUnchanged,
}

/// Decides whether a pushed document replaces the installed one.
///
/// A push without a revision is never stale, so hosts with no ordering
/// guarantee still work; identical content is skipped either way.
#[must_use]
pub fn document_update_decision(
    applied: Option<u64>,
    incoming: Option<u64>,
    unchanged: impl FnOnce() -> bool,
) -> UpdateDecision {
    if let (Some(applied), Some(incoming)) = (applied, incoming)
        && incoming <= applied
    {
        return UpdateDecision::SkipStale;
    }
    if unchanged() {
        return UpdateDecision::SkipUnchanged;
    }
    UpdateDecision::Apply
}

/// A host reply to a repository command. Content pushes are independent.
#[derive(Debug, Deserialize)]
pub struct RepositoryReply {
    pub request_id: u64,
    pub error: Option<String>,
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Default)]
struct RepositoryRequests {
    next: u64,
    pending: Option<u64>,
}

#[cfg(any(target_arch = "wasm32", test))]
impl RepositoryRequests {
    fn begin(&mut self) -> Option<u64> {
        if self.pending.is_some() {
            return None;
        }
        self.next += 1;
        self.pending = Some(self.next);
        self.pending
    }

    fn finish(&mut self, request_id: u64) -> bool {
        if self.pending != Some(request_id) {
            return false;
        }
        self.pending = None;
        true
    }
}

/// Acknowledgement dispatched after every pushed document.
pub const DOCUMENT_APPLIED_EVENT: &str = "diff-review-document-applied";

/// Source-oriented payload used by browser hosts so parsing remains in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownSourcePayload {
    pub source: String,
    pub source_path: Option<String>,
    pub title: Option<String>,
}

/// Decodes and parses a Markdown browser command payload.
///
/// # Errors
/// Returns an error when `json` is not a valid Markdown source payload.
pub fn decode_markdown_document(json: &str) -> Result<MarkdownDocument, WebError> {
    let payload: MarkdownSourcePayload = serde_json::from_str(json).map_err(WebError::InvalidMarkdownPayload)?;
    Ok(MarkdownDocument::parse_with_metadata(payload.source_path, payload.title, payload.source))
}

/// Markdown-specific browser event names; existing diff event names remain unchanged.
pub const MARKDOWN_SET_DOCUMENT_EVENT: &str = "markdown-review-set-document";
pub const MARKDOWN_CLEAR_EVENT: &str = "markdown-review-clear";
pub const MARKDOWN_SUBMIT_EVENT: &str = "markdown-review-submit";
pub const MARKDOWN_COPY_EVENT: &str = "markdown-review-copy";
pub const MARKDOWN_CANCEL_EVENT: &str = "markdown-review-cancel";

/// Returns the checked-in browser demonstration document.
///
/// # Panics
/// Panics if the checked-in JSON fixture is invalid.
#[must_use]
pub fn demo_document() -> DiffDocument {
    decode_document(include_str!("../demo-document.json")).expect("the checked-in web demo document must be valid")
}

/// Resolves an embedded browser theme name.
///
/// # Errors
/// Returns an error when the name is unknown or its theme cannot be parsed.
pub fn decode_theme(name: &str) -> Result<ReviewTheme, WebError> {
    ReviewTheme::builtin(name).map_err(|_| WebError::UnknownTheme(name.into()))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::commands::{
        CAPABILITIES_EVENT, COMMAND_EVENT, COMMAND_RESULT_EVENT, CommandError, CommandRequest, CommandResult,
        ViewerCommand, decode_capabilities, decode_command,
    };
    use super::{
        DOCUMENT_APPLIED_EVENT, MARKDOWN_CANCEL_EVENT, MARKDOWN_CLEAR_EVENT, MARKDOWN_COPY_EVENT,
        MARKDOWN_SET_DOCUMENT_EVENT, MARKDOWN_SUBMIT_EVENT, RepositoryReply, RepositoryRequests, UpdateDecision,
        WebError, decode_document_command, decode_markdown_document, decode_theme, demo_document,
        document_update_decision,
    };
    use async_channel::{Receiver, Sender};
    use clankerdiff_client::{ClientOptions, ClientState, DiffClient};
    use clankerdiff_core::{
        DiffDocument, DiffReviewEvent, LookupReply, RepoPath, ReviewCapabilities, ReviewSubmission, ReviewTarget,
        ViewMode,
    };
    use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewEvent, MarkdownReviewSubmission};
    use clankerdiff_rouge::{
        DiffViewerOptions, MarkdownController, MarkdownHandle, MarkdownOutput, ReviewController, ReviewHandle,
        ReviewOutput, diff_view, markdown_review_view,
    };
    use clankerdiff_ui_core::ReviewTheme;
    use rouge_core::{Color, Styled, View, div, dynamic, view};
    use rouge_reactive::{Signal, Task, on_cleanup, signal, spawn};
    use rouge_widgets::{DEFAULT_INTERFACE_SIZE, InterfaceSize, bundled_fonts};
    use std::{
        cell::{Cell, RefCell},
        sync::Arc,
    };
    use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
    use wasm_bindgen_futures::spawn_local;
    use web_sys::{CustomEvent, CustomEventInit, HtmlCanvasElement};

    const CONNECTION_STATE_EVENT: &str = "diff-review-connection-state";

    thread_local! {
        static COMMANDS: RefCell<Option<Sender<WebCommand>>> = const { RefCell::new(None) };
        static MANAGED: Cell<bool> = const { Cell::new(false) };
        static GENERATION: Cell<u64> = const { Cell::new(0) };
        static APP: RefCell<Option<Mounted>> = const { RefCell::new(None) };
        static STARTING: Cell<bool> = const { Cell::new(false) };
    }

    struct Mounted {
        _app: rouge_web::BrowserApp,
        _listeners: Vec<Listener>,
        canvas: HtmlCanvasElement,
    }

    impl Drop for Mounted {
        fn drop(&mut self) {
            self.canvas.remove();
        }
    }

    struct Listener(web_sys::Document, &'static str, Closure<dyn FnMut(CustomEvent)>);
    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = self.0.remove_event_listener_with_callback(self.1, self.2.as_ref().unchecked_ref());
        }
    }

    enum WebCommand {
        InstallRemote(u64, DiffClient),
        DisconnectRemote,
        RemoteState(u64, Arc<ClientState>),
        RemoteFinished(u64, RepositoryReply),
        LookupFinished(u64, LookupReply),
        Output(ReviewOutput),
        SetDocument { document: Arc<DiffDocument>, revision: Option<u64>, request_id: Option<u64> },
        SetTheme(ReviewTheme),
        SetMarkdownDocument(Arc<MarkdownDocument>),
        RepositoryFinished(RepositoryReply),
        Dispatch(CommandRequest),
        SetCapabilities(ReviewCapabilities),
        ClearReview,
    }

    struct WebRoot {
        remote: Option<(u64, DiffClient)>,
        remote_task: Option<Task>,
        repository_task: Option<Task>,
        lookup_task: Option<Task>,
        capabilities: ReviewCapabilities,
        installed_snapshot: Option<Arc<DiffDocument>>,
        listed_files: Option<Arc<[RepoPath]>>,
        applied_revision: Option<u64>,
        requests: RepositoryRequests,
        viewer: ReviewHandle,
        markdown: Signal<Option<MarkdownHandle>>,
        markdown_scope: Option<rouge_reactive::Scope>,
    }

    fn root(receiver: Receiver<WebCommand>) -> impl View {
        let _ = InterfaceSize::install(DEFAULT_INTERFACE_SIZE);
        let viewer = ReviewController::with_options(
            Arc::new(demo_document()),
            stored_theme(),
            DiffViewerOptions { view_mode: stored_view_mode(), ..DiffViewerOptions::default() },
        )
        .mount(|output| {
            let _ = send(WebCommand::Output(output));
        });
        let markdown = signal(None::<MarkdownHandle>);
        let mut state = WebRoot {
            remote: None,
            remote_task: None,
            repository_task: None,
            lookup_task: None,
            capabilities: ReviewCapabilities::default(),
            installed_snapshot: None,
            listed_files: None,
            applied_revision: None,
            requests: RepositoryRequests::default(),
            viewer: viewer.clone(),
            markdown,
            markdown_scope: None,
        };
        let task = spawn(async move {
            while let Ok(command) = receiver.recv().await {
                state.apply(command);
            }
        });
        on_cleanup(move || task.cancel());
        view! {
            <div size_full>
                <dynamic>
                    {markdown.with(|handle| match handle {
                        Some(handle) => markdown_review_view(handle.clone()).into_any(),
                        None => diff_view(viewer.clone()).into_any(),
                    })}
                </dynamic>
            </div>
        }
    }

    impl Drop for WebRoot {
        fn drop(&mut self) {
            self.drop_remote();
        }
    }

    impl WebRoot {
        fn drop_remote(&mut self) {
            for task in [self.repository_task.take(), self.lookup_task.take()].into_iter().flatten() {
                task.cancel();
            }
            if let Some(task) = self.remote_task.take() {
                task.cancel();
            }
            if let Some((_, client)) = self.remote.take() {
                close(client);
            }
            self.requests.pending = None;
            self.installed_snapshot = None;
            self.listed_files = None;
            self.applied_revision = None;
        }

        fn close_markdown(&mut self) {
            self.markdown.set(None);
            if let Some(scope) = self.markdown_scope.take() {
                scope.dispose();
            }
        }

        fn apply(&mut self, command: WebCommand) {
            match command {
                WebCommand::InstallRemote(id, client) => {
                    if !MANAGED.get() || id != GENERATION.get() {
                        close(client);
                        return;
                    }
                    self.drop_remote();
                    self.close_markdown();
                    let mut updates = client.subscribe();
                    self.remote = Some((id, client));
                    self.remote_state(&updates.latest());
                    self.remote_task = Some(spawn(async move {
                        while let Ok(state) = updates.changed().await {
                            if send(WebCommand::RemoteState(id, state)).is_err() {
                                break;
                            }
                        }
                    }));
                }
                WebCommand::DisconnectRemote => {
                    self.drop_remote();
                    self.viewer.command_finished(Ok(()));
                    self.viewer.set_background_error(None);
                    self.viewer.set_capabilities(self.capabilities);
                    report(emit_connection_state("host", None));
                }
                WebCommand::RemoteState(id, state) if self.is_current(id) => self.remote_state(&state),
                WebCommand::RemoteFinished(id, reply) if self.is_current(id) => self.finish_repository(reply),
                WebCommand::LookupFinished(id, reply) if self.is_current(id) => self.viewer.lookup_finished(reply),
                WebCommand::RemoteState(..) | WebCommand::RemoteFinished(..) | WebCommand::LookupFinished(..) => {}
                WebCommand::Output(output) => self.output(output),
                WebCommand::SetDocument { document, revision, request_id } => {
                    let decision = document_update_decision(self.applied_revision, revision, || {
                        self.viewer.with(|v| v.session().document().as_ref() == document.as_ref())
                    });
                    if decision == UpdateDecision::Apply {
                        self.viewer.set_document(document);
                    }
                    if decision != UpdateDecision::SkipStale {
                        self.close_markdown();
                        self.applied_revision = revision.or(self.applied_revision);
                    }
                    if let Some(request_id) = request_id {
                        self.finish_repository(RepositoryReply { request_id, error: None });
                    }
                    report(dispatch_document_applied(revision, decision == UpdateDecision::Apply));
                }
                WebCommand::SetMarkdownDocument(document) => {
                    if let Some(handle) = self.markdown.get_untracked() {
                        handle.set_document(document);
                    } else {
                        let scope = rouge_reactive::Scope::new();
                        let handle =
                            scope.run(|| MarkdownController::new(document, stored_theme()).mount(markdown_output));
                        handle.set_capabilities(self.capabilities);
                        self.markdown_scope = Some(scope);
                        self.markdown.set(Some(handle));
                    }
                }
                WebCommand::SetTheme(theme) => {
                    if let Some(handle) = self.markdown.get_untracked() {
                        handle.set_theme(theme);
                    } else {
                        self.viewer.set_theme(theme);
                    }
                }
                WebCommand::RepositoryFinished(reply) => self.finish_repository(reply),
                WebCommand::SetCapabilities(capabilities) => {
                    self.capabilities = capabilities;
                    self.viewer.set_capabilities(capabilities);
                    if let Some(handle) = self.markdown.get_untracked() {
                        handle.set_capabilities(capabilities);
                    }
                }
                WebCommand::Dispatch(request) => self.dispatch(request),
                WebCommand::ClearReview => {
                    if let Some(handle) = self.markdown.get_untracked() {
                        handle.clear_review();
                    } else {
                        self.viewer.clear_review();
                    }
                }
            }
        }

        fn is_current(&self, id: u64) -> bool {
            MANAGED.get() && GENERATION.get() == id && self.remote.as_ref().is_some_and(|(active, _)| *active == id)
        }

        fn remote_state(&mut self, state: &ClientState) {
            if let Some(document) = state.snapshot_if_changed(&mut self.installed_snapshot) {
                self.viewer.set_document(document.clone());
            }
            if let Some(files) = state.files_if_changed(&mut self.listed_files) {
                self.viewer.set_repository_files(files);
            }
            let status = state.status();
            self.viewer.set_capabilities(state.capabilities);
            self.viewer.set_background_error(status.clone());
            report(emit_connection_state(state.label(), status.as_deref()));
        }

        fn finish_repository(&mut self, reply: RepositoryReply) {
            if self.requests.finish(reply.request_id) {
                self.viewer.command_finished(reply.error.map_or(Ok(()), Err));
            }
        }

        fn dispatch(&self, request: CommandRequest) {
            let markdown = self.markdown.get_untracked();
            let result = match (request.command, markdown) {
                (ViewerCommand::Diff(command), None) => Ok(self.viewer.handle_command(command)),
                (ViewerCommand::Markdown(command), Some(handle)) => {
                    handle.handle_command(command).map_err(|error| error.to_string())
                }
                _ => Err(CommandError::InactiveReviewer.to_string()),
            };
            let result = CommandResult {
                request_id: request.request_id,
                handled: result.as_ref().copied().unwrap_or(false),
                error: result.err(),
            };
            report(
                serde_json::to_string(&result)
                    .map_err(|error| JsValue::from_str(&error.to_string()))
                    .and_then(|detail| dispatch_custom_event(COMMAND_RESULT_EVENT, Some(&detail))),
            );
        }

        fn output(&mut self, output: ReviewOutput) {
            let event = match output {
                ReviewOutput::ThemeChanged(id) => {
                    store_theme(&id);
                    return;
                }
                ReviewOutput::ViewModeChanged(mode) => {
                    store_view_mode(mode);
                    return;
                }
                ReviewOutput::Event(event) => event,
            };
            if let DiffReviewEvent::Lookup(lookup) = event {
                if let Some((id, client)) = self.remote.clone() {
                    if let Some(task) = self.lookup_task.take() {
                        task.cancel();
                    }
                    self.lookup_task = Some(spawn(async move {
                        let _ = send(WebCommand::LookupFinished(id, client.lookup(lookup).await));
                    }));
                }
                return;
            }
            if MANAGED.get() && !matches!(event, DiffReviewEvent::CopyFormattedReview(_)) {
                if let Some((id, client)) = self.remote.clone()
                    && let Some(request_id) = self.requests.begin()
                {
                    if let Some(task) = self.repository_task.take() {
                        task.cancel();
                    }
                    self.repository_task = Some(spawn(async move {
                        let error = client.handle(event).await.err().map(|error| error.to_string());
                        let _ = send(WebCommand::RemoteFinished(id, RepositoryReply { request_id, error }));
                    }));
                } else {
                    self.viewer.command_finished(Err("Remote connection is not ready".into()));
                }
                return;
            }
            if !matches!(
                event,
                DiffReviewEvent::Refresh | DiffReviewEvent::SetTarget(_) | DiffReviewEvent::RepositoryAction(_)
            ) {
                if let DiffReviewEvent::CopyFormattedReview(text) = &event {
                    copy(text.clone());
                }
                dispatch_viewer_event(&event);
                return;
            }
            let Some(request_id) = self.requests.begin() else {
                return;
            };
            let result = match event {
                DiffReviewEvent::Refresh => dispatch_custom_event(
                    super::REFRESH_REQUEST_EVENT,
                    Some(&serde_json::json!({"request_id": request_id}).to_string()),
                ),
                DiffReviewEvent::SetTarget(target) => dispatch_target_request(&target),
                DiffReviewEvent::RepositoryAction(action) => dispatch_repository_action(request_id, &action),
                _ => return,
            };
            if let Err(error) = result {
                self.finish_repository(RepositoryReply {
                    request_id,
                    error: Some(format!("could not dispatch request: {error:?}")),
                });
            }
        }
    }

    fn report(result: Result<(), JsValue>) {
        if let Err(error) = result {
            web_sys::console::error_1(&error);
        }
    }
    fn copy(text: String) {
        spawn_local(async move {
            report(rouge_web::copy_text(&text).await);
        });
    }
    fn markdown_output(output: MarkdownOutput) {
        match output {
            MarkdownOutput::Event(event) => {
                if let MarkdownReviewEvent::CopyFormatted(text) = &event {
                    copy(text.clone());
                }
                dispatch_markdown_event(&event);
            }
            MarkdownOutput::ThemeChanged(id) => store_theme(&id),
            MarkdownOutput::CopyCode(text) => copy(text),
            MarkdownOutput::OpenLink(url) => {
                if let Some(window) = web_sys::window() {
                    let _ = window.open_with_url_and_target_and_features(&url, "_blank", "noopener,noreferrer");
                }
            }
        }
    }

    #[wasm_bindgen]
    pub fn start() -> Result<(), JsValue> {
        console_error_panic_hook::set_once();
        let receiver = install_command_channel()?;
        let result = prepare();
        let (canvas, listeners) = match result {
            Ok(result) => result,
            Err(error) => {
                COMMANDS.take();
                return Err(error);
            }
        };
        STARTING.set(true);
        connect_from_query();
        spawn_local(async move {
            let app = rouge_web::BrowserApp::mount(
                canvas.clone(),
                bundled_fonts(),
                Color::new(0.06, 0.08, 0.10, 1.0),
                move || root(receiver),
            )
            .await;
            STARTING.set(false);
            match app {
                Ok(app) => {
                    APP.replace(Some(Mounted { _app: app, _listeners: listeners, canvas }));
                }
                Err(error) => {
                    COMMANDS.take();
                    MANAGED.set(false);
                    GENERATION.set(GENERATION.get().wrapping_add(1));
                    canvas.remove();
                    if let Some(doc) = web_sys::window().and_then(|window| window.document())
                        && let Some(body) = doc.body()
                    {
                        body.set_text_content(Some(&format!("Could not start Diff Review: {error}")));
                    }
                    report(Err(JsValue::from_str(&error.to_string())));
                }
            }
        });
        Ok(())
    }

    #[wasm_bindgen]
    pub fn stop() -> Result<(), JsValue> {
        if STARTING.get() {
            return Err(JsValue::from_str("the diff viewer is still starting"));
        }
        COMMANDS.take().ok_or_else(|| js_error(WebError::NotStarted))?;
        MANAGED.set(false);
        GENERATION.set(GENERATION.get().wrapping_add(1));
        drop(APP.take());
        Ok(())
    }

    fn prepare() -> Result<(HtmlCanvasElement, Vec<Listener>), JsValue> {
        let doc = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("browser document is unavailable"))?;
        let canvas = doc.create_element("canvas")?.dyn_into::<HtmlCanvasElement>().map_err(JsValue::from)?;
        canvas.set_attribute("aria-label", "Diff Review")?;
        let mut listeners = Vec::new();
        for (name, command) in [
            ("diff-review-set-document", set_document_json as fn(&str) -> Result<(), JsValue>),
            (MARKDOWN_SET_DOCUMENT_EVENT, set_markdown_document_json),
            ("diff-review-set-theme", set_theme),
            (COMMAND_EVENT, dispatch_command_json),
            (CAPABILITIES_EVENT, set_capabilities_json),
            ("diff-review-repository-completed", complete_repository_json),
        ] {
            listeners.push(listen(&doc, name, move |event| {
                report(
                    event
                        .detail()
                        .as_string()
                        .ok_or_else(|| JsValue::from_str("diff review command detail must be a string"))
                        .and_then(|detail| command(&detail)),
                );
            })?);
        }
        for name in ["diff-review-clear", MARKDOWN_CLEAR_EVENT] {
            listeners.push(listen(&doc, name, |_| report(clear_review()))?);
        }
        doc.body().ok_or_else(|| JsValue::from_str("browser body is unavailable"))?.append_child(&canvas)?;
        Ok((canvas, listeners))
    }

    fn listen(
        doc: &web_sys::Document,
        name: &'static str,
        handle: impl FnMut(CustomEvent) + 'static,
    ) -> Result<Listener, JsValue> {
        let handle = Closure::new(handle);
        doc.add_event_listener_with_callback(name, handle.as_ref().unchecked_ref())?;
        Ok(Listener(doc.clone(), name, handle))
    }

    fn dispatch_viewer_event(event: &DiffReviewEvent) {
        let result = match event {
            DiffReviewEvent::RepositoryAction(_)
            | DiffReviewEvent::SetTarget(_)
            | DiffReviewEvent::Refresh
            | DiffReviewEvent::Lookup(_) => return,
            DiffReviewEvent::SubmitReview(submission) => dispatch_submission(submission),
            DiffReviewEvent::CopyFormattedReview(text) => dispatch_custom_event("diff-review-copy", Some(text)),
            DiffReviewEvent::Cancel => dispatch_custom_event("diff-review-cancel", None),
        };
        if let Err(error) = result {
            web_sys::console::error_1(&error);
        }
    }

    fn dispatch_markdown_event(event: &MarkdownReviewEvent) {
        let result = match event {
            MarkdownReviewEvent::Submit(submission) => dispatch_markdown_submission(submission),
            MarkdownReviewEvent::CopyFormatted(text) => dispatch_custom_event(MARKDOWN_COPY_EVENT, Some(text)),
            MarkdownReviewEvent::Cancel => dispatch_custom_event(MARKDOWN_CANCEL_EVENT, None),
        };
        if let Err(error) = result {
            web_sys::console::error_1(&error);
        }
    }

    fn dispatch_markdown_submission(submission: &MarkdownReviewSubmission) -> Result<(), JsValue> {
        let json = serde_json::to_string(submission)
            .map_err(|error| JsValue::from_str(&format!("failed to serialize Markdown review: {error}")))?;
        dispatch_custom_event(MARKDOWN_SUBMIT_EVENT, Some(&json))
    }

    fn dispatch_document_applied(revision: Option<u64>, changed: bool) -> Result<(), JsValue> {
        let detail = match revision {
            Some(revision) => format!(r#"{{"revision":{revision},"changed":{changed}}}"#),
            None => format!(r#"{{"revision":null,"changed":{changed}}}"#),
        };
        dispatch_custom_event(DOCUMENT_APPLIED_EVENT, Some(&detail))
    }

    fn dispatch_target_request(target: &ReviewTarget) -> Result<(), JsValue> {
        let detail = serde_json::json!({ "target": target }).to_string();
        dispatch_custom_event(super::TARGET_REQUEST_EVENT, Some(&detail))
    }

    fn dispatch_repository_action(request_id: u64, action: &clankerdiff_core::RepositoryAction) -> Result<(), JsValue> {
        let json = serde_json::to_string(&serde_json::json!({ "request_id": request_id, "action": action }))
            .map_err(|error| JsValue::from_str(&format!("failed to serialize repository action: {error}")))?;
        dispatch_custom_event("diff-review-repository-action", Some(&json))
    }

    fn dispatch_submission(submission: &ReviewSubmission) -> Result<(), JsValue> {
        let json = serde_json::to_string(submission)
            .map_err(|error| JsValue::from_str(&format!("failed to serialize review: {error}")))?;
        dispatch_custom_event("diff-review-submit", Some(&json))
    }

    fn dispatch_custom_event(name: &str, detail: Option<&str>) -> Result<(), JsValue> {
        let init = CustomEventInit::new();
        if let Some(detail) = detail {
            init.set_detail(&JsValue::from_str(detail));
        }
        let event = CustomEvent::new_with_event_init_dict(name, &init)?;
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("browser document is unavailable"))?;
        document.dispatch_event(&event).map(|_| ())
    }

    fn send(command: WebCommand) -> Result<(), WebError> {
        COMMANDS.with(|commands| {
            commands
                .borrow()
                .as_ref()
                .ok_or(WebError::NotStarted)?
                .try_send(command)
                .map_err(|_| WebError::CommandChannelClosed)
        })
    }

    const THEME_STORAGE_KEY: &str = "clankerdiff.theme.v1";

    fn stored_theme() -> ReviewTheme {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item(THEME_STORAGE_KEY).ok().flatten())
            .and_then(|id| decode_theme(&id).ok())
            .unwrap_or_default()
    }

    fn store_theme(id: &str) {
        if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
            let _ = storage.set_item(THEME_STORAGE_KEY, id);
        }
    }

    const VIEW_MODE_STORAGE_KEY: &str = "clankerdiff.view_mode.v1";

    fn stored_view_mode() -> ViewMode {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item(VIEW_MODE_STORAGE_KEY).ok().flatten())
            .and_then(|mode| serde_json::from_str(&mode).ok())
            .unwrap_or_default()
    }

    fn store_view_mode(mode: ViewMode) {
        if let (Some(storage), Ok(mode)) =
            (web_sys::window().and_then(|window| window.local_storage().ok().flatten()), serde_json::to_string(&mode))
        {
            let _ = storage.set_item(VIEW_MODE_STORAGE_KEY, &mode);
        }
    }

    // Matches Result::map_err's owned error callback.
    #[allow(clippy::needless_pass_by_value)]
    fn js_error(error: WebError) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn install_command_channel() -> Result<Receiver<WebCommand>, JsValue> {
        let (sender, receiver) = async_channel::unbounded();
        COMMANDS.with(|commands| {
            if commands.borrow().is_some() {
                return Err(JsValue::from_str("the diff viewer is already started"));
            }
            *commands.borrow_mut() = Some(sender);
            Ok(())
        })?;
        Ok(receiver)
    }

    fn connect_from_query() {
        if let Some(url) = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .and_then(|query| web_sys::UrlSearchParams::new_with_str(&query).ok())
            .and_then(|params| params.get("connect"))
        {
            spawn_local(async move {
                if let Err(error) = connect_remote(&url).await {
                    web_sys::console::error_1(&error);
                }
            });
        }
    }

    /// Pushes a document, as a bare `DiffDocument` or a revision envelope.
    ///
    /// Pushes are idempotent and the result is acknowledged on
    /// `diff-review-document-applied`.
    ///
    /// # Errors
    /// Returns an error for invalid JSON or an unavailable command channel.
    #[wasm_bindgen]
    pub fn set_document_json(json: &str) -> Result<(), JsValue> {
        ensure_host_mode()?;
        let mut command = decode_document_command(json).map_err(js_error)?;
        if let Some(scope) = command.scope {
            command.document.target = scope.into();
        }
        send(WebCommand::SetDocument {
            document: Arc::new(command.document),
            revision: command.revision,
            request_id: command.request_id,
        })
        .map_err(js_error)
    }

    /// Switches the root to rendered Markdown and parses the source payload in Rust.
    ///
    /// # Errors
    /// Returns an error for an invalid payload or an unavailable command channel.
    #[wasm_bindgen]
    pub fn set_markdown_document_json(json: &str) -> Result<(), JsValue> {
        ensure_host_mode()?;
        let document = decode_markdown_document(json).map_err(js_error)?;
        send(WebCommand::SetMarkdownDocument(Arc::new(document))).map_err(js_error)
    }

    /// Completes a specific repository command, even if its content is unchanged.
    ///
    /// # Errors
    /// Returns an error for an invalid reply or an unavailable command channel.
    #[wasm_bindgen]
    pub fn complete_repository_json(json: &str) -> Result<(), JsValue> {
        ensure_host_mode()?;
        let reply =
            serde_json::from_str::<RepositoryReply>(json).map_err(|error| JsValue::from_str(&error.to_string()))?;
        send(WebCommand::RepositoryFinished(reply)).map_err(js_error)
    }

    /// Selects one of the themes returned by the built-in theme catalog.
    ///
    /// # Errors
    /// Returns an error for an unknown theme or an unavailable command channel.
    #[wasm_bindgen]
    pub fn set_theme(name: &str) -> Result<(), JsValue> {
        let theme = decode_theme(name).map_err(js_error)?;
        store_theme(&theme.id().to_string());
        send(WebCommand::SetTheme(theme)).map_err(js_error)
    }

    /// Removes every queued review comment and active draft.
    ///
    /// # Errors
    /// Returns an error if the command channel is unavailable.
    #[wasm_bindgen]
    pub fn clear_review() -> Result<(), JsValue> {
        send(WebCommand::ClearReview).map_err(js_error)
    }

    fn close(client: DiffClient) {
        spawn_local(async move {
            let _ = client.close().await;
        });
    }

    fn emit_connection_state(state: &str, error: Option<&str>) -> Result<(), JsValue> {
        let detail = serde_json::json!({ "state": state, "error": error }).to_string();
        dispatch_custom_event(CONNECTION_STATE_EVENT, Some(&detail))
    }

    fn ensure_host_mode() -> Result<(), JsValue> {
        if MANAGED.get() {
            Err(JsValue::from_str("managed remote connection owns the document; disconnect_remote first"))
        } else {
            Ok(())
        }
    }

    #[wasm_bindgen]
    pub async fn connect_remote(url: &str) -> Result<(), JsValue> {
        ensure_host_mode()?;
        if !COMMANDS.with(|commands| commands.borrow().is_some()) {
            return Err(js_error(WebError::NotStarted));
        }
        let id = GENERATION.get().wrapping_add(1);
        GENERATION.set(id);
        MANAGED.set(true);
        match DiffClient::connect(url, ClientOptions::default()).await {
            Ok(client) => {
                if GENERATION.get() != id {
                    let _ = client.close().await;
                    return Err(JsValue::from_str("remote connection was superseded"));
                }
                if let Err(error) = send(WebCommand::InstallRemote(id, client.clone())) {
                    MANAGED.set(false);
                    let _ = client.close().await;
                    return Err(js_error(error));
                }
                Ok(())
            }
            Err(error) => {
                if GENERATION.get() != id {
                    return Err(JsValue::from_str("remote connection was superseded"));
                }
                MANAGED.set(false);
                let message = error.to_string();
                let _ = emit_connection_state("failed", Some(&message));
                Err(JsValue::from_str(&error.to_string()))
            }
        }
    }

    #[wasm_bindgen]
    pub fn disconnect_remote() -> Result<(), JsValue> {
        MANAGED.set(false);
        GENERATION.set(GENERATION.get().wrapping_add(1));
        send(WebCommand::DisconnectRemote).map_err(js_error)
    }

    #[wasm_bindgen]
    pub fn dispatch_command_json(json: &str) -> Result<(), JsValue> {
        let request = decode_command(json).map_err(|error| JsValue::from_str(&error.to_string()))?;
        send(WebCommand::Dispatch(request)).map_err(js_error)
    }

    #[wasm_bindgen]
    pub fn set_capabilities_json(json: &str) -> Result<(), JsValue> {
        ensure_host_mode()?;
        let capabilities = decode_capabilities(json).map_err(|error| JsValue::from_str(&error.to_string()))?;
        send(WebCommand::SetCapabilities(capabilities)).map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{
    clear_review, complete_repository_json, connect_remote, disconnect_remote, dispatch_command_json,
    set_capabilities_json, set_document_json, set_markdown_document_json, set_theme, start, stop,
};

#[cfg(test)]
mod tests {
    use super::*;
    use clankerdiff_ui_core::ThemeId;

    #[test]
    fn completion_is_request_scoped_and_independent_of_content() {
        let mut requests = RepositoryRequests::default();
        let first = requests.begin().unwrap();
        assert!(requests.begin().is_none(), "only one command owns pending");
        for unchanged in [false, true] {
            let _ = document_update_decision(None, None, || unchanged);
            assert_eq!(requests.pending, Some(first), "pushes do not complete commands");
        }
        let decision = document_update_decision(Some(2), Some(3), || true);
        assert_eq!(decision, UpdateDecision::SkipUnchanged);
        assert!(requests.finish(first), "an unchanged success completes its command");
        let second = requests.begin().unwrap();
        assert!(!requests.finish(first), "duplicate/late replies cannot clear the next command");
        assert_eq!(requests.pending, Some(second));
        assert!(requests.finish(second));
    }

    #[test]
    fn document_envelopes_carry_optional_scope() -> Result<(), WebError> {
        let bare = decode_document_command(r#"{"repo_root":"/fixture","files":[]}"#)?;
        assert_eq!(bare.scope, None);
        let unstaged = decode_document_command(
            r#"{"revision":4,"scope":"unstaged","document":{"repo_root":"/fixture","files":[]}}"#,
        )?;
        assert_eq!(unstaged.scope, Some(clankerdiff_core::DiffScope::Unstaged));
        let staged = decode_document_command(
            r#"{"revision":4,"scope":"staged","document":{"repo_root":"/fixture","files":[]}}"#,
        )?;
        assert_eq!(staged.scope, Some(clankerdiff_core::DiffScope::Staged));
        let both =
            decode_document_command(r#"{"revision":4,"scope":"both","document":{"repo_root":"/fixture","files":[]}}"#)?;
        assert_eq!(both.scope, Some(clankerdiff_core::DiffScope::Both));
        assert!(matches!(
            decode_document_command(r#"{"scope":"invalid","document":{"repo_root":"/fixture","files":[]}}"#),
            Err(WebError::InvalidScope(_))
        ));
        assert_eq!(TARGET_REQUEST_EVENT, "diff-review-set-target");
        Ok(())
    }

    #[test]
    fn document_envelopes_can_complete_a_specific_command() {
        let command =
            decode_document_command(r#"{"revision":4,"request_id":7,"document":{"repo_root":"/fixture","files":[]}}"#)
                .unwrap();
        assert_eq!(command.request_id, Some(7));
        assert_eq!(command.revision, Some(4));
        let reply: RepositoryReply = serde_json::from_str(r#"{"request_id":7,"error":"git failed"}"#).unwrap();
        assert_eq!(reply.request_id, 7);
        assert_eq!(reply.error.as_deref(), Some("git failed"));
    }

    #[test]
    fn bundled_demo_is_the_captured_workspace_diff() {
        let document = demo_document();
        assert_eq!(document.repo_root, ".");
        assert_ne!(document.files.len(), 0);
    }

    #[test]
    fn decodes_host_document_boundary() {
        let document = decode_document(r#"{"repo_root":"/fixture","files":[]}"#).unwrap();
        assert_eq!(document.repo_root, "/fixture");
        assert_eq!(document.files.len(), 0);
    }

    #[test]
    fn decodes_both_bare_documents_and_revision_envelopes() {
        let bare = decode_document_command(r#"{"repo_root":"/fixture","files":[]}"#).unwrap();
        assert_eq!(bare.revision, None);
        assert_eq!(bare.document.repo_root, "/fixture");

        let envelope =
            decode_document_command(r#"{"revision":4,"document":{"repo_root":"/fixture","files":[]}}"#).unwrap();
        assert_eq!(envelope.revision, Some(4));
        assert_eq!(envelope.document.repo_root, "/fixture");
    }

    #[test]
    fn stale_pushes_do_not_inspect_document_content() {
        assert_eq!(
            document_update_decision(Some(3), Some(2), || panic!("stale content must not be compared")),
            UpdateDecision::SkipStale
        );
    }

    #[test]
    fn pushes_are_idempotent_and_ordered_when_revisions_are_supplied() {
        assert_eq!(document_update_decision(None, None, || false), UpdateDecision::Apply);
        assert_eq!(document_update_decision(None, None, || true), UpdateDecision::SkipUnchanged);
        assert_eq!(document_update_decision(Some(2), Some(3), || false), UpdateDecision::Apply);
        assert_eq!(document_update_decision(Some(3), Some(3), || false), UpdateDecision::SkipStale);
        assert_eq!(document_update_decision(Some(3), Some(2), || false), UpdateDecision::SkipStale);
        assert_eq!(
            document_update_decision(Some(3), None, || false),
            UpdateDecision::Apply,
            "a host without revisions is never stale"
        );
        assert_eq!(document_update_decision(Some(2), Some(3), || true), UpdateDecision::SkipUnchanged);
    }

    #[test]
    fn rejects_malformed_document() {
        assert!(matches!(decode_document("not json"), Err(WebError::InvalidDocument(_))));
    }

    #[test]
    fn decodes_markdown_source_payload_in_rust() {
        let document =
            decode_markdown_document(r##"{"source":"# Plan","source_path":"plan.md","title":"Review"}"##).unwrap();
        assert_eq!(document.source(), "# Plan");
        assert_eq!(document.source_path(), Some("plan.md"));
        assert_eq!(document.title(), Some("Review"));
        assert_eq!(document.outline()[0].title, "Plan");
        assert!(matches!(decode_markdown_document("not json"), Err(WebError::InvalidMarkdownPayload(_))));
    }

    #[test]
    fn markdown_event_names_do_not_overlap_diff_events() {
        assert_eq!(MARKDOWN_SUBMIT_EVENT, "markdown-review-submit");
        assert_ne!(MARKDOWN_SUBMIT_EVENT, "diff-review-submit");
        assert_eq!(MARKDOWN_SET_DOCUMENT_EVENT, "markdown-review-set-document");
    }

    #[test]
    fn accepts_embedded_theme_names() {
        assert_eq!(decode_theme("sage").unwrap().id(), &ThemeId::Sage);
        assert_eq!(decode_theme("ayu-dark").unwrap().id(), &ThemeId::Ayu);
        assert_eq!(decode_theme("tokyo-night").unwrap().id().to_string(), "tokyo-night");
        assert!(matches!(decode_theme("unknown"), Err(WebError::UnknownTheme(_))));
    }
}
