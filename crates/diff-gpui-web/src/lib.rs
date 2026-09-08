use clankerdiff_core::{DiffDocument, DiffScope};
use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_theme::DiffTheme;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

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
    /// The GPUI command channel has not been installed yet.
    #[error("the diff viewer has not started")]
    NotStarted,
    /// The GPUI command channel is no longer accepting commands.
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
        DocumentPayload::Envelope {
            revision,
            request_id,
            scope,
            document,
        } => {
            let scope = scope
                .map(|value| DiffScope::from_str(&value).map_err(|_| WebError::InvalidScope(value)))
                .transpose()?;
            Ok(DocumentCommand {
                revision,
                request_id,
                scope,
                document,
            })
        }
        DocumentPayload::Bare(document) => Ok(DocumentCommand {
            revision: None,
            request_id: None,
            scope: None,
            document,
        }),
    }
}

/// Outbound event dispatched when the viewer requests a scope change.
pub const SCOPE_REQUEST_EVENT: &str = "diff-review-set-scope";

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
    let payload: MarkdownSourcePayload =
        serde_json::from_str(json).map_err(WebError::InvalidMarkdownPayload)?;
    Ok(MarkdownDocument::parse_with_metadata(
        payload.source_path,
        payload.title,
        payload.source,
    ))
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
    decode_document(include_str!("../demo-document.json"))
        .expect("the checked-in web demo document must be valid")
}

/// Resolves an embedded browser theme name.
///
/// # Errors
/// Returns an error when the name is unknown or its theme cannot be parsed.
pub fn decode_theme(name: &str) -> Result<DiffTheme, WebError> {
    DiffTheme::builtin(name).map_err(|_| WebError::UnknownTheme(name.into()))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{
        DOCUMENT_APPLIED_EVENT, MARKDOWN_CANCEL_EVENT, MARKDOWN_CLEAR_EVENT, MARKDOWN_COPY_EVENT,
        MARKDOWN_SET_DOCUMENT_EVENT, MARKDOWN_SUBMIT_EVENT, RepositoryReply, RepositoryRequests,
        UpdateDecision, WebError, decode_document_command, decode_markdown_document, decode_theme,
        demo_document, document_update_decision,
    };
    use async_channel::{Receiver, Sender};
    use clankerdiff_core::{DiffDocument, DiffReviewEvent, ReviewSubmission};
    use clankerdiff_gpui::{
        DiffViewer, DiffViewerOptions, MarkdownReviewer, MarkdownReviewerOptions, ThemeChanged,
        load_default_fonts,
    };
    use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewEvent, MarkdownReviewSubmission};
    use clankerdiff_theme::DiffTheme;
    use gpui::{
        App, AppContext, ApplicationHandle, Bounds, Context, Entity, Render, Subscription, Task,
        Window, WindowBounds, WindowOptions, prelude::*, px, size,
    };
    use std::{cell::RefCell, rc::Rc, sync::Arc};
    use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
    use web_sys::{CustomEvent, CustomEventInit};

    thread_local! {
        static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
        static COMMANDS: RefCell<Option<Sender<WebCommand>>> = const { RefCell::new(None) };
    }

    enum WebCommand {
        SetDocument {
            document: Arc<DiffDocument>,
            revision: Option<u64>,
            request_id: Option<u64>,
            scope: Option<DiffScope>,
        },
        SetTheme(DiffTheme),
        SetMarkdownDocument(Arc<MarkdownDocument>),
        RepositoryFinished(RepositoryReply),
        ClearReview,
    }

    struct WebRoot {
        applied_revision: Option<u64>,
        current_scope: Option<DiffScope>,
        requests: RepositoryRequests,
        viewer: Entity<DiffViewer>,
        _viewer_subscription: Subscription,
        _viewer_theme_subscription: Subscription,
        markdown: Option<Entity<MarkdownReviewer>>,
        markdown_subscription: Option<Subscription>,
        markdown_theme_subscription: Option<Subscription>,
        _command_task: Task<()>,
    }

    impl WebRoot {
        fn new(receiver: Receiver<WebCommand>, cx: &mut Context<Self>) -> Self {
            let theme = stored_theme();
            let viewer = cx.new(|_| {
                DiffViewer::with_options(
                    Arc::new(demo_document()),
                    theme,
                    DiffViewerOptions::default(),
                )
            });
            let viewer_subscription = cx.subscribe(
                &viewer,
                |this: &mut Self, _viewer, event: &DiffReviewEvent, cx| match event {
                    DiffReviewEvent::RepositoryAction(action) => this
                        .request_repository(cx, |request_id| {
                            dispatch_repository_action(request_id, action)
                        }),
                    DiffReviewEvent::SetScope(scope) => {
                        this.request_repository(cx, |_| dispatch_scope_request(*scope))
                    }
                    _ => dispatch_viewer_event(event),
                },
            );
            let viewer_theme_subscription =
                cx.subscribe(&viewer, |_this, _viewer, event: &ThemeChanged, _cx| {
                    store_theme(&event.id);
                });
            let command_task = cx.spawn(async move |this, cx| {
                while let Ok(command) = receiver.recv().await {
                    if this
                        .update(cx, |root, cx| root.apply_command(command, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            });

            Self {
                applied_revision: None,
                current_scope: None,
                requests: RepositoryRequests::default(),
                viewer,
                _viewer_subscription: viewer_subscription,
                _viewer_theme_subscription: viewer_theme_subscription,
                markdown: None,
                markdown_subscription: None,
                markdown_theme_subscription: None,
                _command_task: command_task,
            }
        }

        fn finish_repository(&mut self, reply: RepositoryReply, cx: &mut Context<Self>) {
            if self.requests.finish(reply.request_id) {
                self.viewer.update(cx, |viewer, cx| match reply.error {
                    Some(message) => viewer.set_repository_error(message, cx),
                    None => viewer.set_repository_pending(false, cx),
                });
            }
        }

        fn request_repository(
            &mut self,
            cx: &mut Context<Self>,
            dispatch: impl FnOnce(u64) -> Result<(), JsValue>,
        ) {
            let Some(request_id) = self.requests.begin() else {
                return;
            };
            self.viewer
                .update(cx, |viewer, cx| viewer.set_repository_pending(true, cx));
            if let Err(error) = dispatch(request_id) {
                self.finish_repository(
                    RepositoryReply {
                        request_id,
                        error: Some(format!("could not dispatch request: {error:?}")),
                    },
                    cx,
                );
            }
        }

        fn apply_command(&mut self, command: WebCommand, cx: &mut Context<Self>) {
            match command {
                WebCommand::SetDocument {
                    document,
                    revision,
                    request_id,
                    scope,
                } => {
                    let decision =
                        document_update_decision(self.applied_revision, revision, || {
                            self.viewer.read(cx).document().as_ref() == document.as_ref()
                        });
                    if decision == UpdateDecision::Apply {
                        self.viewer.update(cx, |viewer, cx| {
                            if let Some(scope) = scope {
                                viewer.set_scope(scope, cx);
                            }
                            viewer.set_document(document, cx);
                        });
                    }
                    if decision != UpdateDecision::SkipStale
                        && let Some(scope) = scope
                    {
                        self.current_scope = Some(scope);
                    }
                    if decision != UpdateDecision::SkipStale {
                        self.markdown = None;
                        self.markdown_subscription = None;
                        self.markdown_theme_subscription = None;
                        // An unchanged push still advances the host's ordering.
                        self.applied_revision = revision.or(self.applied_revision);
                    }
                    if let Some(request_id) = request_id {
                        self.finish_repository(
                            RepositoryReply {
                                request_id,
                                error: None,
                            },
                            cx,
                        );
                    }
                    if let Err(error) =
                        dispatch_document_applied(revision, decision == UpdateDecision::Apply)
                    {
                        web_sys::console::error_1(&error);
                    }
                }
                WebCommand::SetMarkdownDocument(document) => {
                    if let Some(markdown) = &self.markdown {
                        markdown.update(cx, |reviewer, cx| reviewer.set_document(document, cx));
                    } else {
                        let theme = stored_theme();
                        let markdown = cx.new(|_| {
                            MarkdownReviewer::with_options(
                                document,
                                theme,
                                MarkdownReviewerOptions::default(),
                            )
                        });
                        self.markdown_subscription = Some(cx.subscribe(
                            &markdown,
                            |_this, _reviewer, event: &MarkdownReviewEvent, _cx| {
                                dispatch_markdown_event(event);
                            },
                        ));
                        self.markdown_theme_subscription = Some(cx.subscribe(
                            &markdown,
                            |_this, _reviewer, event: &ThemeChanged, _cx| store_theme(&event.id),
                        ));
                        self.markdown = Some(markdown);
                    }
                }
                WebCommand::SetTheme(theme) => {
                    if let Some(markdown) = &self.markdown {
                        markdown.update(cx, |reviewer, cx| reviewer.set_theme(theme, cx));
                    } else {
                        self.viewer
                            .update(cx, |viewer, cx| viewer.set_theme(theme, cx));
                    }
                }
                WebCommand::RepositoryFinished(reply) => self.finish_repository(reply, cx),
                WebCommand::ClearReview => {
                    if let Some(markdown) = &self.markdown {
                        markdown.update(cx, MarkdownReviewer::clear_review);
                    } else {
                        self.viewer.update(cx, DiffViewer::clear_review);
                    }
                }
            }
            cx.notify();
        }
    }

    impl Render for WebRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            self.markdown.as_ref().map_or_else(
                || self.viewer.clone().into_any_element(),
                |reviewer| reviewer.clone().into_any_element(),
            )
        }
    }

    fn dispatch_viewer_event(event: &DiffReviewEvent) {
        let result = match event {
            DiffReviewEvent::RepositoryAction(_) | DiffReviewEvent::SetScope(_) => return, // handled by the root
            DiffReviewEvent::SubmitReview(submission) => dispatch_submission(submission),
            DiffReviewEvent::CopyFormattedReview(text) => {
                dispatch_custom_event("diff-review-copy", Some(text))
            }
            DiffReviewEvent::Cancel => dispatch_custom_event("diff-review-cancel", None),
        };
        if let Err(error) = result {
            web_sys::console::error_1(&error);
        }
    }

    fn dispatch_markdown_event(event: &MarkdownReviewEvent) {
        let result = match event {
            MarkdownReviewEvent::Submit(submission) => dispatch_markdown_submission(submission),
            MarkdownReviewEvent::CopyFormatted(text) => {
                dispatch_custom_event(MARKDOWN_COPY_EVENT, Some(text))
            }
            MarkdownReviewEvent::Cancel => dispatch_custom_event(MARKDOWN_CANCEL_EVENT, None),
        };
        if let Err(error) = result {
            web_sys::console::error_1(&error);
        }
    }

    fn dispatch_markdown_submission(submission: &MarkdownReviewSubmission) -> Result<(), JsValue> {
        let json = serde_json::to_string(submission).map_err(|error| {
            JsValue::from_str(&format!("failed to serialize Markdown review: {error}"))
        })?;
        dispatch_custom_event(MARKDOWN_SUBMIT_EVENT, Some(&json))
    }

    fn dispatch_document_applied(revision: Option<u64>, changed: bool) -> Result<(), JsValue> {
        let detail = match revision {
            Some(revision) => format!(r#"{{"revision":{revision},"changed":{changed}}}"#),
            None => format!(r#"{{"revision":null,"changed":{changed}}}"#),
        };
        dispatch_custom_event(DOCUMENT_APPLIED_EVENT, Some(&detail))
    }

    fn dispatch_scope_request(scope: DiffScope) -> Result<(), JsValue> {
        let detail = serde_json::json!({ "scope": scope.as_str() }).to_string();
        dispatch_custom_event(super::SCOPE_REQUEST_EVENT, Some(&detail))
    }

    fn dispatch_repository_action(
        request_id: u64,
        action: &clankerdiff_core::RepositoryAction,
    ) -> Result<(), JsValue> {
        let json = serde_json::to_string(
            &serde_json::json!({ "request_id": request_id, "action": action }),
        )
        .map_err(|error| {
            JsValue::from_str(&format!("failed to serialize repository action: {error}"))
        })?;
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

    fn stored_theme() -> DiffTheme {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item(THEME_STORAGE_KEY).ok().flatten())
            .and_then(|id| decode_theme(&id).ok())
            .unwrap_or_default()
    }

    fn store_theme(id: &str) {
        if let Some(storage) =
            web_sys::window().and_then(|window| window.local_storage().ok().flatten())
        {
            let _ = storage.set_item(THEME_STORAGE_KEY, id);
        }
    }

    // Matches Result::map_err's owned error callback.
    #[allow(clippy::needless_pass_by_value)]
    fn js_error(error: WebError) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn single_threaded_web() -> gpui::Application {
        let platform = Rc::new(gpui_web::WebPlatform::new(false));
        let http_client = Arc::new(platform.fetch_http_client());
        gpui::Application::with_platform(platform).with_http_client(http_client)
    }

    fn install_host_event_listeners() -> Result<(), JsValue> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("browser document is unavailable"))?;

        install_string_command(&document, "diff-review-set-document", set_document_json)?;
        install_string_command(
            &document,
            MARKDOWN_SET_DOCUMENT_EVENT,
            set_markdown_document_json,
        )?;
        install_string_command(&document, "diff-review-set-theme", set_theme)?;
        install_string_command(
            &document,
            "diff-review-repository-completed",
            complete_repository_json,
        )?;

        let clear = Closure::<dyn FnMut(CustomEvent)>::new(|_event: CustomEvent| {
            if let Err(error) = clear_review() {
                web_sys::console::error_1(&error);
            }
        });
        document.add_event_listener_with_callback(
            "diff-review-clear",
            clear.as_ref().unchecked_ref(),
        )?;
        clear.forget();

        let clear_markdown = Closure::<dyn FnMut(CustomEvent)>::new(|_event: CustomEvent| {
            if let Err(error) = clear_review() {
                web_sys::console::error_1(&error);
            }
        });
        document.add_event_listener_with_callback(
            MARKDOWN_CLEAR_EVENT,
            clear_markdown.as_ref().unchecked_ref(),
        )?;
        clear_markdown.forget();
        Ok(())
    }

    fn install_string_command(
        document: &web_sys::Document,
        event_name: &str,
        command: fn(&str) -> Result<(), JsValue>,
    ) -> Result<(), JsValue> {
        let listener = Closure::<dyn FnMut(CustomEvent)>::new(move |event: CustomEvent| {
            let Some(value) = event.detail().as_string() else {
                web_sys::console::error_1(&JsValue::from_str(
                    "diff review command detail must be a string",
                ));
                return;
            };
            if let Err(error) = command(&value) {
                web_sys::console::error_1(&error);
            }
        });
        document.add_event_listener_with_callback(event_name, listener.as_ref().unchecked_ref())?;
        listener.forget();
        Ok(())
    }

    /// Initializes the single-threaded GPUI browser platform and its one canvas.
    ///
    /// # Errors
    /// Returns an error if already started or browser listeners cannot be installed.
    ///
    /// # Panics
    /// Panics if bundled fonts cannot load or the canvas cannot be opened.
    #[wasm_bindgen(start)]
    pub fn start() -> Result<(), JsValue> {
        console_error_panic_hook::set_once();
        gpui_web::init_logging();
        let (sender, receiver) = async_channel::unbounded();
        COMMANDS.with(|commands| {
            if commands.borrow().is_some() {
                return Err(JsValue::from_str("the diff viewer is already started"));
            }
            *commands.borrow_mut() = Some(sender);
            Ok(())
        })?;
        install_host_event_listeners()?;

        let application = single_threaded_web().run_embedded(move |cx: &mut App| {
            load_default_fonts(cx).expect("failed to load the bundled fonts");
            DiffViewer::bind_keys(cx);
            MarkdownReviewer::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    focus: true,
                    ..Default::default()
                },
                |_window, cx| cx.new(|cx| WebRoot::new(receiver, cx)),
            )
            .expect("GPUI web must be able to open its document-owned canvas");
            cx.activate(true);
        });
        APPLICATION.with(|current| *current.borrow_mut() = Some(application));
        Ok(())
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
        let command = decode_document_command(json).map_err(js_error)?;
        send(WebCommand::SetDocument {
            document: Arc::new(command.document),
            revision: command.revision,
            request_id: command.request_id,
            scope: command.scope,
        })
        .map_err(js_error)
    }

    /// Switches the root to rendered Markdown and parses the source payload in Rust.
    ///
    /// # Errors
    /// Returns an error for an invalid payload or an unavailable command channel.
    #[wasm_bindgen]
    pub fn set_markdown_document_json(json: &str) -> Result<(), JsValue> {
        let document = decode_markdown_document(json).map_err(js_error)?;
        send(WebCommand::SetMarkdownDocument(Arc::new(document))).map_err(js_error)
    }

    /// Completes a specific repository command, even if its content is unchanged.
    ///
    /// # Errors
    /// Returns an error for an invalid reply or an unavailable command channel.
    #[wasm_bindgen]
    pub fn complete_repository_json(json: &str) -> Result<(), JsValue> {
        let reply = serde_json::from_str::<RepositoryReply>(json)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
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
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{
    clear_review, complete_repository_json, set_document_json, set_markdown_document_json,
    set_theme, start,
};

#[cfg(test)]
mod tests {
    use super::*;
    use clankerdiff_theme::ThemeId;

    #[test]
    fn completion_is_request_scoped_and_independent_of_content() {
        let mut requests = RepositoryRequests::default();
        let first = requests.begin().unwrap();
        assert!(requests.begin().is_none(), "only one command owns pending");
        for unchanged in [false, true] {
            let _ = document_update_decision(None, None, || unchanged);
            assert_eq!(
                requests.pending,
                Some(first),
                "pushes do not complete commands"
            );
        }
        let decision = document_update_decision(Some(2), Some(3), || true);
        assert_eq!(decision, UpdateDecision::SkipUnchanged);
        assert!(
            requests.finish(first),
            "an unchanged success completes its command"
        );
        let second = requests.begin().unwrap();
        assert!(
            !requests.finish(first),
            "duplicate/late replies cannot clear the next command"
        );
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
        let both = decode_document_command(
            r#"{"revision":4,"scope":"both","document":{"repo_root":"/fixture","files":[]}}"#,
        )?;
        assert_eq!(both.scope, Some(clankerdiff_core::DiffScope::Both));
        assert!(matches!(
            decode_document_command(
                r#"{"scope":"invalid","document":{"repo_root":"/fixture","files":[]}}"#
            ),
            Err(WebError::InvalidScope(_))
        ));
        assert_eq!(SCOPE_REQUEST_EVENT, "diff-review-set-scope");
        Ok(())
    }

    #[test]
    fn document_envelopes_can_complete_a_specific_command() {
        let command = decode_document_command(
            r#"{"revision":4,"request_id":7,"document":{"repo_root":"/fixture","files":[]}}"#,
        )
        .unwrap();
        assert_eq!(command.request_id, Some(7));
        assert_eq!(command.revision, Some(4));
        let reply: RepositoryReply =
            serde_json::from_str(r#"{"request_id":7,"error":"git failed"}"#).unwrap();
        assert_eq!(reply.request_id, 7);
        assert_eq!(reply.error.as_deref(), Some("git failed"));
    }

    #[test]
    fn bundled_demo_is_the_captured_workspace_diff() {
        let document = demo_document();
        assert_eq!(document.repo_root, ".");
        assert!(!document.files.is_empty());
    }

    #[test]
    fn decodes_host_document_boundary() {
        let document = decode_document(r#"{"repo_root":"/fixture","files":[]}"#).unwrap();
        assert_eq!(document.repo_root, "/fixture");
        assert!(document.files.is_empty());
    }

    #[test]
    fn decodes_both_bare_documents_and_revision_envelopes() {
        let bare = decode_document_command(r#"{"repo_root":"/fixture","files":[]}"#).unwrap();
        assert_eq!(bare.revision, None);
        assert_eq!(bare.document.repo_root, "/fixture");

        let envelope = decode_document_command(
            r#"{"revision":4,"document":{"repo_root":"/fixture","files":[]}}"#,
        )
        .unwrap();
        assert_eq!(envelope.revision, Some(4));
        assert_eq!(envelope.document.repo_root, "/fixture");
    }

    #[test]
    fn stale_pushes_do_not_inspect_document_content() {
        assert_eq!(
            document_update_decision(Some(3), Some(2), || panic!(
                "stale content must not be compared"
            )),
            UpdateDecision::SkipStale
        );
    }

    #[test]
    fn pushes_are_idempotent_and_ordered_when_revisions_are_supplied() {
        assert_eq!(
            document_update_decision(None, None, || false),
            UpdateDecision::Apply
        );
        assert_eq!(
            document_update_decision(None, None, || true),
            UpdateDecision::SkipUnchanged
        );
        assert_eq!(
            document_update_decision(Some(2), Some(3), || false),
            UpdateDecision::Apply
        );
        assert_eq!(
            document_update_decision(Some(3), Some(3), || false),
            UpdateDecision::SkipStale
        );
        assert_eq!(
            document_update_decision(Some(3), Some(2), || false),
            UpdateDecision::SkipStale
        );
        assert_eq!(
            document_update_decision(Some(3), None, || false),
            UpdateDecision::Apply,
            "a host without revisions is never stale"
        );
        assert_eq!(
            document_update_decision(Some(2), Some(3), || true),
            UpdateDecision::SkipUnchanged
        );
    }

    #[test]
    fn rejects_malformed_document() {
        assert!(matches!(
            decode_document("not json"),
            Err(WebError::InvalidDocument(_))
        ));
    }

    #[test]
    fn decodes_markdown_source_payload_in_rust() {
        let document = decode_markdown_document(
            r##"{"source":"# Plan","source_path":"plan.md","title":"Review"}"##,
        )
        .unwrap();
        assert_eq!(document.source(), "# Plan");
        assert_eq!(document.source_path(), Some("plan.md"));
        assert_eq!(document.title(), Some("Review"));
        assert_eq!(document.outline()[0].title, "Plan");
        assert!(matches!(
            decode_markdown_document("not json"),
            Err(WebError::InvalidMarkdownPayload(_))
        ));
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
        assert_eq!(
            decode_theme("tokyo-night").unwrap().id().to_string(),
            "tokyo-night"
        );
        assert!(matches!(
            decode_theme("unknown"),
            Err(WebError::UnknownTheme(_))
        ));
    }
}
