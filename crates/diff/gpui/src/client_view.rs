use crate::{
    DiffViewer, DiffViewerOptions, ThemeChanged, ViewModeChanged,
    ui::prelude::{EmptyState, NoticeTone, UiTheme},
};
use clankerdiff_client::{ClientState, ConnectionState};
use clankerdiff_core::{DiffDocument, DiffReviewCommand, DiffReviewEvent, Lookup, LookupReply, ReviewSubmission};
use clankerdiff_ui_core::ReviewTheme;
use gpui::{AnyElement, ClipboardItem, Context, Entity, EventEmitter, Subscription, Window, div, prelude::*};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum DiffClientEvent {
    Command(DiffReviewEvent),
    Lookup(Lookup),
    Submit(ReviewSubmission),
    ReviewChanged { comments: usize },
    Cancel,
}

pub struct DiffClientView {
    theme: ReviewTheme,
    options: DiffViewerOptions,
    state: Option<ClientState>,
    installed: Option<Arc<DiffDocument>>,
    viewer: Option<Entity<DiffViewer>>,
    comments: usize,
    pending: bool,
    subscriptions: Vec<Subscription>,
}

impl DiffClientView {
    #[must_use]
    pub fn new(theme: ReviewTheme) -> Self {
        Self::with_options(theme, DiffViewerOptions::default())
    }

    #[must_use]
    pub const fn with_options(theme: ReviewTheme, options: DiffViewerOptions) -> Self {
        Self {
            theme,
            options,
            state: None,
            installed: None,
            viewer: None,
            comments: 0,
            pending: false,
            subscriptions: Vec::new(),
        }
    }

    #[must_use]
    pub const fn theme(&self) -> &ReviewTheme {
        &self.theme
    }

    pub fn set_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        if let Some(viewer) = &self.viewer {
            let theme = theme.clone();
            viewer.update(cx, |viewer, cx| viewer.set_theme(theme, cx));
        }
        self.theme = theme;
        cx.notify();
    }

    pub fn set_state(&mut self, state: &ClientState, cx: &mut Context<Self>) {
        if let Some(document) = state.snapshot_if_changed(&mut self.installed) {
            self.install(document.clone(), cx);
        }
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.set_capabilities(state.capabilities, cx);
                viewer.set_background_error(state.status(), cx);
            });
        }
        self.state = Some(state.clone());
        cx.notify();
    }

    pub fn command_finished(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.pending = false;
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| match result {
                Ok(()) => viewer.set_repository_pending(false, cx),
                Err(error) => viewer.set_repository_error(error, cx),
            });
        }
    }

    pub fn lookup_finished(&mut self, reply: LookupReply, cx: &mut Context<Self>) {
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| viewer.lookup_finished(reply, cx));
        }
    }

    pub fn clear_review(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, DiffViewer::clear_review);
        }
    }

    pub fn handle_command(&mut self, command: DiffReviewCommand, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.viewer
            .as_ref()
            .is_some_and(|viewer| viewer.update(cx, |viewer, cx| viewer.handle_command(command, window, cx)))
    }

    fn install(&mut self, document: Arc<DiffDocument>, cx: &mut Context<Self>) {
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| viewer.set_document(document, cx));
            return;
        }
        let (theme, options) = (self.theme.clone(), self.options);
        let viewer = cx.new(|_| DiffViewer::with_options(document, theme, options));
        self.subscriptions.extend([
            cx.subscribe(&viewer, |this, _, event: &DiffReviewEvent, cx| {
                this.on_viewer_event(event.clone(), cx);
            }),
            cx.subscribe(&viewer, |this, viewer, event: &ThemeChanged, cx| {
                this.theme = viewer.read(cx).theme().clone();
                cx.emit(event.clone());
                cx.notify();
            }),
            cx.subscribe(&viewer, |_, _, event: &ViewModeChanged, cx| {
                cx.emit(*event);
            }),
            cx.observe(&viewer, |this, viewer, cx| {
                let comments = viewer.read(cx).review().len();
                if comments != this.comments {
                    this.comments = comments;
                    cx.emit(DiffClientEvent::ReviewChanged { comments });
                }
            }),
        ]);
        self.viewer = Some(viewer);
    }

    fn on_viewer_event(&mut self, event: DiffReviewEvent, cx: &mut Context<Self>) {
        match event {
            DiffReviewEvent::SubmitReview(submission) => {
                cx.emit(DiffClientEvent::Submit(submission));
            }
            DiffReviewEvent::CopyFormattedReview(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            DiffReviewEvent::Cancel => cx.emit(DiffClientEvent::Cancel),
            DiffReviewEvent::Lookup(lookup) => cx.emit(DiffClientEvent::Lookup(lookup)),
            event @ (DiffReviewEvent::RepositoryAction(_)
            | DiffReviewEvent::SetTarget(_)
            | DiffReviewEvent::Refresh) => self.send(event, cx),
        }
    }

    fn send(&mut self, event: DiffReviewEvent, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.pending, true) {
            return;
        }
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| viewer.set_repository_pending(true, cx));
        }
        cx.emit(DiffClientEvent::Command(event));
    }

    fn render_status(&self, title: &'static str, detail: String, tone: NoticeTone) -> AnyElement {
        div()
            .debug_selector(|| "changes-status".to_owned())
            .size_full()
            .child(EmptyState::new(title, detail, tone, UiTheme::new(&self.theme)))
            .into_any_element()
    }
}

impl EventEmitter<DiffClientEvent> for DiffClientView {}

impl EventEmitter<ThemeChanged> for DiffClientView {}

impl EventEmitter<ViewModeChanged> for DiffClientView {}

impl Render for DiffClientView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(state) = &self.state else {
            return self.render_status("Loading changes…", String::new(), NoticeTone::Neutral);
        };
        if let (Some(_), Some(viewer)) = (&state.snapshot, &self.viewer) {
            return div()
                .debug_selector(|| "changes-viewer".to_owned())
                .size_full()
                .child(viewer.clone())
                .into_any_element();
        }
        let detail = state.status().unwrap_or_default();
        if matches!(state.connection, ConnectionState::Failed(_)) {
            self.render_status("Could not load changes", detail, NoticeTone::Error)
        } else {
            self.render_status("Loading changes…", detail, NoticeTone::Neutral)
        }
    }
}
