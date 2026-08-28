//! Browser host boundary for [`diff_gpui::DiffViewer`].
//!
//! The shell accepts serialized [`diff_core::DiffDocument`] snapshots from
//! JavaScript and never accesses Git, the filesystem, or native processes.
//! Review submission is dispatched on `document` as a `diff-review-submit`
//! `CustomEvent`; its `detail` is a serialized `ReviewSubmission` JSON string.

use diff_core::{DiffDocument, DiffTheme};

/// Errors returned while validating commands from JavaScript.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// The supplied document was not valid `DiffDocument` JSON.
    #[error("invalid diff document JSON: {0}")]
    InvalidDocument(#[from] serde_json::Error),
    /// The selected embedded theme is not available.
    #[error("unknown theme `{0}`; expected `sage` or `ayu-dark`")]
    UnknownTheme(String),
    /// The GPUI command channel has not been installed yet.
    #[error("the diff viewer has not started")]
    NotStarted,
    /// The GPUI command channel is no longer accepting commands.
    #[error("the diff viewer command channel is closed")]
    CommandChannelClosed,
}

pub fn decode_document(json: &str) -> Result<DiffDocument, WebError> {
    serde_json::from_str(json).map_err(WebError::from)
}

#[must_use]
pub fn demo_document() -> DiffDocument {
    decode_document(include_str!("../demo-document.json"))
        .expect("the checked-in web demo document must be valid")
}

pub fn decode_theme(name: &str) -> Result<DiffTheme, WebError> {
    match name.trim().to_ascii_lowercase().as_str() {
        "sage" => Ok(DiffTheme::default()),
        "ayu" | "ayu-dark" => DiffTheme::ayu().map_err(|_| WebError::UnknownTheme(name.into())),
        _ => Err(WebError::UnknownTheme(name.into())),
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{WebError, decode_document, decode_theme, demo_document};
    use async_channel::{Receiver, Sender};
    use diff_core::{DiffDocument, DiffReviewEvent, DiffTheme, ReviewSubmission};
    use diff_gpui::DiffViewer;
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
        ClearReview,
    }

    struct WebRoot {
        viewer: Entity<DiffViewer>,
        _viewer_subscription: Subscription,
        _command_task: Task<()>,
    }

    impl WebRoot {
        fn new(receiver: Receiver<WebCommand>, cx: &mut Context<Self>) -> Self {
            let viewer = cx.new(|_| DiffViewer::new(Arc::new(demo_document())));
            let viewer_subscription = cx
                .subscribe(&viewer, |_this, _viewer, event: &DiffReviewEvent, _cx| {
                    dispatch_viewer_event(event)
                });
            let weak_viewer = viewer.downgrade();
            let command_task = cx.spawn(async move |_this, cx| {
                while let Ok(command) = receiver.recv().await {
                    if weak_viewer
                        .update(cx, |viewer, cx| apply_command(viewer, command, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            });

            Self {
                viewer,
                _viewer_subscription: viewer_subscription,
                _command_task: command_task,
            }
        }
    }

    impl Render for WebRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            self.viewer.clone()
        }
    }

    fn apply_command(viewer: &mut DiffViewer, command: WebCommand, cx: &mut Context<DiffViewer>) {
        match command {
            WebCommand::SetDocument(document) => viewer.set_document(document, cx),
            WebCommand::SetTheme(theme) => viewer.set_theme(theme, cx),
            WebCommand::ClearReview => viewer.clear_review(cx),
        }
    }

    fn dispatch_viewer_event(event: &DiffReviewEvent) {
        let result = match event {
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
        install_string_command(&document, "diff-review-set-theme", set_theme)?;

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
            DiffViewer::bind_keys(cx);
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

    /// Selects an embedded theme (`sage`, `ayu`, or `ayu-dark`).
    #[wasm_bindgen]
    pub fn set_theme(name: &str) -> Result<(), JsValue> {
        let theme = decode_theme(name).map_err(js_error)?;
        send(WebCommand::SetTheme(theme)).map_err(js_error)
    }

    /// Removes every queued review comment and active draft.
    #[wasm_bindgen]
    pub fn clear_review() -> Result<(), JsValue> {
        send(WebCommand::ClearReview).map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{clear_review, set_document_json, set_theme, start};

#[cfg(test)]
mod tests {
    use super::*;

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
    fn accepts_embedded_theme_names() {
        assert!(decode_theme("sage").is_ok());
        assert!(decode_theme("ayu-dark").is_ok());
        assert!(matches!(
            decode_theme("unknown"),
            Err(WebError::UnknownTheme(_))
        ));
    }
}
