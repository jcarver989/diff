use crate::{
    DiffViewer, DiffViewerOptions, ThemeChanged,
    ui::prelude::{Button, ControlSize, EmptyState, NoticeTone, UiTheme},
};
use clankerdiff_client::{ClientState, ConnectionState, DiffSnapshot};
use clankerdiff_core::{DiffReviewCommand, DiffReviewEvent, DiffScope, ReviewSubmission};
use clankerdiff_theme::ReviewTheme;
use gpui::{
    AnyElement, ClipboardItem, Context, Entity, EventEmitter, Subscription, Window, div, prelude::*,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum DiffClientEvent {
    Command(DiffReviewEvent),
    Submit(ReviewSubmission),
    Cancel,
}

pub struct DiffClientView {
    theme: ReviewTheme,
    state: Option<ClientState>,
    installed: Option<Arc<DiffSnapshot>>,
    viewer: Option<Entity<DiffViewer>>,
    pending: bool,
    subscriptions: Vec<Subscription>,
}

impl DiffClientView {
    #[must_use]
    pub const fn new(theme: ReviewTheme) -> Self {
        Self {
            theme,
            state: None,
            installed: None,
            viewer: None,
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
        if let Some(snapshot) = state.snapshot_if_changed(&mut self.installed) {
            self.install(snapshot, cx);
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

    pub fn clear_review(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, DiffViewer::clear_review);
        }
    }

    pub fn handle_command(
        &mut self,
        command: DiffReviewCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.viewer.as_ref().is_some_and(|viewer| {
            viewer.update(cx, |viewer, cx| viewer.handle_command(command, window, cx))
        })
    }

    fn install(&mut self, snapshot: &DiffSnapshot, cx: &mut Context<Self>) {
        let document = snapshot.document.clone();
        let scope = snapshot.scope;
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.set_scope(scope, cx);
                viewer.set_document(document, cx);
            });
            return;
        }
        let theme = self.theme.clone();
        let viewer = cx.new(|cx| {
            let mut viewer =
                DiffViewer::with_options(document, theme, DiffViewerOptions::default());
            viewer.set_scope(scope, cx);
            viewer
        });
        self.subscriptions.extend([
            cx.subscribe(&viewer, |this, _, event: &DiffReviewEvent, cx| {
                this.on_viewer_event(event.clone(), cx);
            }),
            cx.subscribe(&viewer, |this, viewer, event: &ThemeChanged, cx| {
                this.theme = viewer.read(cx).theme().clone();
                cx.emit(event.clone());
                cx.notify();
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
            event @ (DiffReviewEvent::RepositoryAction(_)
            | DiffReviewEvent::SetScope(_)
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

    fn render_clean(&self, scope: DiffScope, cx: &mut Context<Self>) -> AnyElement {
        let theme = UiTheme::new(&self.theme);
        let segment = |id: &'static str, label: &'static str, value: DiffScope| {
            let enabled = self.viewer.as_ref().is_some_and(|viewer| {
                viewer
                    .read(cx)
                    .command_enabled(&DiffReviewCommand::SetScope(value))
            });
            Button::new(id, label, theme)
                .size(ControlSize::Small)
                .selected(scope == value)
                .disabled(!enabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if scope != value {
                        this.send(DiffReviewEvent::SetScope(value), cx);
                    }
                }))
        };
        div()
            .debug_selector(|| "changes-clean".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .child(div().flex_none().child(EmptyState::new(
                "No changes",
                format!("Nothing to review in {scope}"),
                NoticeTone::Neutral,
                theme,
            )))
            .child(div().flex().gap_1().children([
                segment("scope-unstaged", "Unstaged", DiffScope::Unstaged),
                segment("scope-staged", "Staged", DiffScope::Staged),
                segment("scope-both", "Both", DiffScope::Both),
            ]))
            .into_any_element()
    }

    fn render_status(&self, title: &'static str, detail: String, tone: NoticeTone) -> AnyElement {
        div()
            .debug_selector(|| "changes-status".to_owned())
            .size_full()
            .child(EmptyState::new(
                title,
                detail,
                tone,
                UiTheme::new(&self.theme),
            ))
            .into_any_element()
    }
}

impl EventEmitter<DiffClientEvent> for DiffClientView {}

impl EventEmitter<ThemeChanged> for DiffClientView {}

impl Render for DiffClientView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(state) = &self.state else {
            return self.render_status("Loading changes…", String::new(), NoticeTone::Neutral);
        };
        match (&state.snapshot, &self.viewer) {
            (Some(snapshot), _) if snapshot.document.files.is_empty() => {
                self.render_clean(snapshot.scope, cx)
            }
            (Some(_), Some(viewer)) => div()
                .debug_selector(|| "changes-viewer".to_owned())
                .size_full()
                .child(viewer.clone())
                .into_any_element(),
            _ => {
                let detail = state.status().unwrap_or_default();
                if matches!(state.connection, ConnectionState::Failed(_)) {
                    self.render_status("Could not load changes", detail, NoticeTone::Error)
                } else {
                    self.render_status("Loading changes…", detail, NoticeTone::Neutral)
                }
            }
        }
    }
}
