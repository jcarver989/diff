//! Browser host boundary for [`diff_gpui::DiffViewer`].
//!
//! The shell accepts serialized [`diff_core::DiffDocument`] snapshots from
//! JavaScript and never accesses Git, the filesystem, or native processes.
//! Review submission is dispatched on `document` as a `diff-review-submit`
//! `CustomEvent`; its `detail` is a serialized `ReviewSubmission` JSON string.

use diff_core::{DiffDocument, SourceResponse};
use diff_markdown::MarkdownDocument;
use diff_theme::DiffTheme;
use serde::{Deserialize, Serialize};

/// Errors returned while validating commands from JavaScript.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// The supplied document was not valid `DiffDocument` JSON.
    #[error("invalid diff document JSON: {0}")]
    InvalidDocument(#[from] serde_json::Error),
    /// The supplied Markdown source payload was malformed.
    #[error("invalid Markdown document payload: {0}")]
    InvalidMarkdownPayload(serde_json::Error),
    /// A source response payload was malformed.
    #[error("invalid source response JSON: {0}")]
    InvalidSourceResponse(serde_json::Error),
    /// The selected embedded theme is not available.
    #[error("unknown built-in theme `{0}`")]
    UnknownTheme(String),
    /// The GPUI command channel has not been installed yet.
    #[error("the diff viewer has not started")]
    NotStarted,
    /// The GPUI command channel is no longer accepting commands.
    #[error("the diff viewer command channel is closed")]
    CommandChannelClosed,
}

/// Decodes a serialized diff document command.
///
/// # Errors
/// Returns an error when `json` is not a valid diff document.
pub fn decode_document(json: &str) -> Result<DiffDocument, WebError> {
    serde_json::from_str(json).map_err(WebError::from)
}

/// Source-oriented payload used by browser hosts so parsing remains in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownSourcePayload {
    pub source: String,
    pub source_path: Option<String>,
    pub title: Option<String>,
}

/// Decodes a complete-source response from a browser host.
///
/// # Errors
/// Returns an error when `json` is not a valid source response.
pub fn decode_source_response(json: &str) -> Result<SourceResponse, WebError> {
    serde_json::from_str(json).map_err(WebError::InvalidSourceResponse)
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
pub const SOURCE_REQUEST_EVENT: &str = "diff-review-source-request";
pub const SOURCE_RESPONSE_EVENT: &str = "diff-review-source-response";
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
        MARKDOWN_CANCEL_EVENT, MARKDOWN_CLEAR_EVENT, MARKDOWN_COPY_EVENT,
        MARKDOWN_SET_DOCUMENT_EVENT, MARKDOWN_SUBMIT_EVENT, SOURCE_REQUEST_EVENT,
        SOURCE_RESPONSE_EVENT, WebError, decode_document, decode_markdown_document,
        decode_source_response, decode_theme, demo_document,
    };
    use async_channel::{Receiver, Sender};
    use diff_core::{DiffDocument, DiffReviewEvent, ReviewSubmission, SourceResponse};
    use diff_gpui::{
        DiffViewer, DiffViewerOptions, MarkdownReviewer, MarkdownReviewerOptions, SourceRequested,
        ThemeChanged, load_default_fonts,
    };
    use diff_markdown::{MarkdownDocument, MarkdownReviewEvent, MarkdownReviewSubmission};
    use diff_theme::DiffTheme;
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
        SetDocument(Arc<DiffDocument>),
        SetTheme(DiffTheme),
        SetMarkdownDocument(Arc<MarkdownDocument>),
        RepositoryPending,
        RepositoryError(String),
        ClearReview,
        ProvideSource(SourceResponse),
    }

    struct WebRoot {
        viewer: Entity<DiffViewer>,
        _viewer_subscription: Subscription,
        _viewer_theme_subscription: Subscription,
        _source_subscription: Subscription,
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
            let viewer_subscription = cx
                .subscribe(&viewer, |_this, _viewer, event: &DiffReviewEvent, _cx| {
                    dispatch_viewer_event(event)
                });
            let viewer_theme_subscription = cx
                .subscribe(&viewer, |_this, _viewer, event: &ThemeChanged, _cx| {
                    store_theme(&event.id)
                });
            let source_subscription =
                cx.subscribe(&viewer, |_this, _viewer, event: &SourceRequested, _cx| {
                    for request in &event.requests {
                        if let Ok(json) = serde_json::to_string(request) {
                            let _ = dispatch_custom_event(SOURCE_REQUEST_EVENT, Some(&json));
                        }
                    }
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
                viewer,
                _viewer_subscription: viewer_subscription,
                _viewer_theme_subscription: viewer_theme_subscription,
                _source_subscription: source_subscription,
                markdown: None,
                markdown_subscription: None,
                markdown_theme_subscription: None,
                _command_task: command_task,
            }
        }

        fn apply_command(&mut self, command: WebCommand, cx: &mut Context<Self>) {
            match command {
                WebCommand::SetDocument(document) => {
                    self.markdown = None;
                    self.markdown_subscription = None;
                    self.markdown_theme_subscription = None;
                    self.viewer
                        .update(cx, |viewer, cx| viewer.set_document(document, cx));
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
                WebCommand::RepositoryPending => {
                    self.viewer
                        .update(cx, |viewer, cx| viewer.set_repository_pending(true, cx));
                }
                WebCommand::RepositoryError(message) => {
                    self.viewer
                        .update(cx, |viewer, cx| viewer.set_repository_error(message, cx));
                }
                WebCommand::ProvideSource(response) => {
                    self.viewer.update(cx, |viewer, cx| {
                        viewer.provide_source(response, cx);
                    });
                }
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
            DiffReviewEvent::RepositoryAction(action) => {
                if let Err(error) = send(WebCommand::RepositoryPending) {
                    return web_sys::console::error_1(&js_error(error));
                }
                dispatch_repository_action(action)
            }
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

    fn dispatch_repository_action(action: &diff_core::RepositoryAction) -> Result<(), JsValue> {
        let json = serde_json::to_string(action).map_err(|error| {
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
        install_string_command(&document, SOURCE_RESPONSE_EVENT, provide_source_json)?;
        install_string_command(
            &document,
            "diff-review-repository-error",
            set_repository_error,
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

    /// Replaces the viewer snapshot with a serialized `DiffDocument`.
    #[wasm_bindgen]
    pub fn set_document_json(json: &str) -> Result<(), JsValue> {
        let document = decode_document(json).map_err(js_error)?;
        send(WebCommand::SetDocument(Arc::new(document))).map_err(js_error)
    }

    /// Switches the root to rendered Markdown and parses the source payload in Rust.
    #[wasm_bindgen]
    pub fn set_markdown_document_json(json: &str) -> Result<(), JsValue> {
        let document = decode_markdown_document(json).map_err(js_error)?;
        send(WebCommand::SetMarkdownDocument(Arc::new(document))).map_err(js_error)
    }

    fn set_repository_error(message: &str) -> Result<(), JsValue> {
        send(WebCommand::RepositoryError(message.to_owned())).map_err(js_error)
    }

    #[wasm_bindgen]
    pub fn provide_source_json(json: &str) -> Result<(), JsValue> {
        let response = decode_source_response(json).map_err(js_error)?;
        send(WebCommand::ProvideSource(response)).map_err(js_error)
    }

    /// Selects one of the themes returned by the built-in theme catalog.
    #[wasm_bindgen]
    pub fn set_theme(name: &str) -> Result<(), JsValue> {
        let theme = decode_theme(name).map_err(js_error)?;
        store_theme(&theme.id().to_string());
        send(WebCommand::SetTheme(theme)).map_err(js_error)
    }

    /// Removes every queued review comment and active draft.
    #[wasm_bindgen]
    pub fn clear_review() -> Result<(), JsValue> {
        send(WebCommand::ClearReview).map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{
    clear_review, provide_source_json, set_document_json, set_markdown_document_json, set_theme,
    start,
};

#[cfg(test)]
mod tests {
    use super::*;
    use diff_core::{DiffSide, RepoPath, SourceKey};
    use diff_theme::ThemeId;
    use std::sync::Arc;

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
    fn source_response_decoder_preserves_snapshot_coordinates_and_rejects_bad_json() {
        let expected = SourceResponse {
            epoch: 9,
            key: SourceKey {
                review_path: RepoPath::new("current.rs").unwrap(),
                side: DiffSide::Old,
            },
            result: Ok(Arc::from("old source\n")),
        };
        let json = serde_json::to_string(&expected).unwrap();
        assert_eq!(decode_source_response(&json).unwrap(), expected);
        assert!(matches!(
            decode_source_response(r#"{"epoch":"wrong"}"#),
            Err(WebError::InvalidSourceResponse(_))
        ));
        assert_eq!(SOURCE_REQUEST_EVENT, "diff-review-source-request");
        assert_eq!(SOURCE_RESPONSE_EVENT, "diff-review-source-response");
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
