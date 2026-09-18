#![allow(missing_docs)] // GPUI action declarations cannot carry per-action documentation.

use crate::{args::CliArgs, menus::SetScope, preferences, window_chrome};
use clankerdiff_client::{ClientState, DiffClient, DiffSnapshot};
use clankerdiff_core::{
    DiffReviewCommand, DiffReviewEvent, DiffScope, ReviewCapabilities, ReviewSubmission,
};
use clankerdiff_gpui::{
    DEFAULT_FONT_FAMILY, DiffViewer, DiffViewerOptions, StageAll, ThemeChanged, UnstageAll,
    ui::prelude::{EmptyState, NoticeTone, UiTheme},
};
use clankerdiff_server::{DiffServer, ServerOptions};
use clankerdiff_theme::ReviewTheme;
use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, KeyBinding, Subscription, Task, Window, div,
    prelude::*,
};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc::Sender},
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum LoadState {
    Loading,
    Error(String),
    Empty,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HostEventEffect {
    None,
    Copy(String),
    PrintSubmission(String),
    Quit,
}

fn host_event_effect(event: &DiffReviewEvent) -> HostEventEffect {
    match event {
        DiffReviewEvent::RepositoryAction(_)
        | DiffReviewEvent::SetScope(_)
        | DiffReviewEvent::Refresh => HostEventEffect::None,
        DiffReviewEvent::CopyFormattedReview(text) => HostEventEffect::Copy(text.clone()),
        DiffReviewEvent::SubmitReview(submission) => HostEventEffect::PrintSubmission(
            serde_json::to_string_pretty(submission)
                .unwrap_or_else(|error| format!("{{\"serialization_error\":\"{error}\"}}")),
        ),
        DiffReviewEvent::Cancel => HostEventEffect::Quit,
    }
}

pub(crate) struct DesktopApp {
    repository_path: PathBuf,
    client: Option<DiffClient>,
    server: Option<DiffServer>,
    scope: DiffScope,
    installed_snapshot: Option<Arc<DiffSnapshot>>,
    state: LoadState,
    viewer: Option<Entity<DiffViewer>>,
    viewer_subscription: Option<Subscription>,
    theme_subscription: Option<Subscription>,
    theme: ReviewTheme,
    load_task: Option<Task<()>>,
    command_task: Option<Task<()>>,
    watch_task: Task<()>,
    outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
}

impl DesktopApp {
    pub(crate) fn new(
        args: CliArgs,
        client: Option<DiffClient>,
        outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut app = Self {
            repository_path: args.repository,
            client: None,
            server: None,
            scope: args.scope,
            installed_snapshot: None,
            state: LoadState::Loading,
            viewer: None,
            viewer_subscription: None,
            theme_subscription: None,
            theme: preferences::load_theme(),
            load_task: None,
            command_task: None,
            watch_task: Task::ready(()),
            outcome_sender,
        };
        if let Some(client) = client {
            app.install_client(client, cx);
        } else {
            app.discover(cx);
        }
        app
    }

    pub(crate) fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("cmd-shift-s", StageAll, Some("DesktopDiff")),
            KeyBinding::new("ctrl-shift-s", StageAll, Some("DesktopDiff")),
            KeyBinding::new("cmd-shift-u", UnstageAll, Some("DesktopDiff")),
            KeyBinding::new("ctrl-shift-u", UnstageAll, Some("DesktopDiff")),
        ]);
    }

    fn discover(&mut self, cx: &mut Context<Self>) {
        if self.load_task.is_some() {
            return;
        }
        let path = self.repository_path.clone();
        let scope = self.scope;
        self.state = LoadState::Loading;
        let operation = gpui_tokio::Tokio::spawn(cx, async move {
            let server = DiffServer::open(path, ServerOptions::default())
                .await
                .map_err(|error| error.to_string())?;
            let transport = server.connect().map_err(|error| error.to_string())?;
            let client = DiffClient::from_transport(transport, scope.into())
                .await
                .map_err(|error| error.to_string())?;
            Ok::<_, String>((server, client))
        });
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| {
                this.load_task = None;
                match result {
                    Ok(Ok((server, client))) => {
                        this.server = Some(server);
                        this.install_client(client, cx);
                    }
                    Ok(Err(error)) => this.set_error(&error, cx),
                    Err(error) => this.set_error(&format!("background task failed: {error}"), cx),
                }
            });
        }));
        cx.notify();
    }

    fn install_client(&mut self, client: DiffClient, cx: &mut Context<Self>) {
        let mut updates = client.subscribe();
        self.apply_client_state(&updates.latest(), cx);
        self.client = Some(client);
        self.watch_task = cx.spawn(async move |this, cx| {
            while let Ok(state) = updates.changed().await {
                if this
                    .update(cx, |this, cx| this.apply_client_state(&state, cx))
                    .is_err()
                {
                    return;
                }
            }
        });
    }

    fn apply_client_state(&mut self, state: &ClientState, cx: &mut Context<Self>) {
        if let Some(snapshot) = state.snapshot_if_changed(&mut self.installed_snapshot) {
            self.scope = snapshot.scope;
            self.install_snapshot(snapshot, cx);
        }
        let error = state.status();
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.set_capabilities(state.capabilities, cx);
                viewer.set_background_error(error, cx);
            });
        } else if let Some(error) = error {
            self.set_error(&error, cx);
        }
    }

    fn command(&mut self, event: DiffReviewEvent, cx: &mut Context<Self>) {
        // A single command owns pending state; snapshots never settle it.
        if self.command_task.is_some() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| viewer.set_repository_pending(true, cx));
        }
        let operation = gpui_tokio::Tokio::spawn(cx, async move {
            client
                .handle(event)
                .await
                .map_err(|error| error.to_string())
        });
        self.command_task = Some(cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| {
                this.command_task = None;
                match result {
                    Ok(Ok(())) => {
                        if let Some(viewer) = &this.viewer {
                            viewer
                                .update(cx, |viewer, cx| viewer.set_repository_pending(false, cx));
                        }
                    }
                    Ok(Err(error)) => this.report_error(&error, cx),
                    Err(error) => {
                        this.report_error(&format!("background task failed: {error}"), cx);
                    }
                }
            });
        }));
    }

    fn install_snapshot(&mut self, snapshot: &DiffSnapshot, cx: &mut Context<Self>) {
        let is_empty = snapshot.document.files.is_empty();
        let document = snapshot.document.clone();
        let scope = snapshot.scope;
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.set_scope(scope, cx);
                viewer.set_document(document, cx);
            });
        } else {
            let theme = self.theme.clone();
            let scope = snapshot.scope;
            let viewer = cx.new(|cx| {
                let mut viewer =
                    DiffViewer::with_options(document, theme, DiffViewerOptions::default());
                viewer.set_scope(scope, cx);
                viewer.set_capabilities(ReviewCapabilities::default(), cx);
                viewer
            });
            self.viewer_subscription = Some(cx.subscribe(
                &viewer,
                |this, _viewer, event: &DiffReviewEvent, cx| {
                    this.handle_viewer_event(event, cx);
                },
            ));
            self.theme_subscription = Some(cx.subscribe(
                &viewer,
                |this, _viewer, event: &ThemeChanged, _cx| {
                    if let Ok(theme) = ReviewTheme::builtin(&event.id) {
                        this.theme = theme;
                    }
                    let _ = preferences::save_theme(&event.id);
                },
            ));
            self.viewer = Some(viewer);
        }
        self.state = if is_empty {
            LoadState::Empty
        } else {
            LoadState::Ready
        };
        cx.notify();
    }

    fn set_error(&mut self, message: &str, cx: &mut Context<Self>) {
        self.state = LoadState::Error(message.to_owned());
        cx.notify();
    }

    /// Reports a failure without discarding the snapshot already on screen.
    fn report_error(&mut self, message: &str, cx: &mut Context<Self>) {
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.set_repository_error(message.to_owned(), cx);
            });
            return;
        }
        self.set_error(message, cx);
    }

    fn handle_viewer_event(&mut self, event: &DiffReviewEvent, cx: &mut Context<Self>) {
        match event {
            DiffReviewEvent::RepositoryAction(_)
            | DiffReviewEvent::SetScope(_)
            | DiffReviewEvent::Refresh => {
                self.command(event.clone(), cx);
                return;
            }
            _ => {}
        }
        if let Some(sender) = &self.outcome_sender {
            match event {
                DiffReviewEvent::RepositoryAction(_)
                | DiffReviewEvent::SetScope(_)
                | DiffReviewEvent::Refresh => {
                    unreachable!("handled above")
                }
                DiffReviewEvent::CopyFormattedReview(text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                }
                DiffReviewEvent::SubmitReview(submission) => {
                    let _ = sender.send(Some(submission.clone()));
                    cx.quit();
                }
                DiffReviewEvent::Cancel => {
                    let _ = sender.send(None);
                    cx.quit();
                }
            }
            return;
        }

        match host_event_effect(event) {
            HostEventEffect::None => {}
            HostEventEffect::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            HostEventEffect::PrintSubmission(json) => println!("{json}"),
            HostEventEffect::Quit => cx.quit(),
        }
    }

    fn dispatch_command(
        &mut self,
        command: DiffReviewCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.handle_command(command, window, cx);
            });
        }
    }

    fn stage_all(&mut self, _: &StageAll, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_command(DiffReviewCommand::StageAll, window, cx);
    }

    fn unstage_all(&mut self, _: &UnstageAll, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_command(DiffReviewCommand::UnstageAll, window, cx);
    }

    fn menu_set_scope(&mut self, action: &SetScope, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_command(DiffReviewCommand::SetScope(action.scope()), window, cx);
    }

    fn render_empty(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        use clankerdiff_gpui::ui::prelude::{Button, ControlSize};
        let scope = self.scope;
        let viewer = self.viewer.clone();
        let theme = UiTheme::new(&self.theme);
        let segment = |id: &'static str, label: &'static str, value: DiffScope| {
            Button::new(id, label, theme)
                .size(ControlSize::Small)
                .selected(scope == value)
                .disabled(!viewer.as_ref().is_some_and(|viewer| {
                    viewer
                        .read(cx)
                        .command_enabled(&DiffReviewCommand::SetScope(value))
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.dispatch_command(DiffReviewCommand::SetScope(value), window, cx);
                }))
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .gap_2()
            .child(self.status_panel(
                "No changes",
                &format!("Scope: {scope} · press S to change scope"),
                NoticeTone::Neutral,
            ))
            .child(div().flex().gap_2().children([
                segment("scope-unstaged", "Unstaged", DiffScope::Unstaged),
                segment("scope-staged", "Staged", DiffScope::Staged),
                segment("scope-both", "Both", DiffScope::Both),
            ]))
    }

    fn status_panel(&self, title: &str, detail: &str, tone: NoticeTone) -> impl IntoElement {
        EmptyState::new(
            title.to_owned(),
            detail.to_owned(),
            tone,
            UiTheme::new(&self.theme),
        )
    }
}

impl Render for DesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = div()
            .key_context("DesktopDiff")
            .on_action(cx.listener(Self::menu_set_scope))
            .on_action(cx.listener(Self::stage_all))
            .on_action(cx.listener(Self::unstage_all))
            .size_full()
            .font_family(DEFAULT_FONT_FAMILY)
            .child(match &self.state {
                LoadState::Loading => self
                    .status_panel(
                        "Loading diff…",
                        "Git is reading the repository",
                        NoticeTone::Info,
                    )
                    .into_any_element(),
                LoadState::Error(error) => self
                    .status_panel(
                        "Could not load diff",
                        &format!("{error} · press ⌘/Ctrl+R to retry"),
                        NoticeTone::Error,
                    )
                    .into_any_element(),
                LoadState::Empty => self.render_empty(cx).into_any_element(),
                LoadState::Ready => self.viewer.as_ref().map_or_else(
                    || {
                        self.status_panel(
                            "No viewer",
                            "Press ⌘/Ctrl+R to retry",
                            NoticeTone::Warning,
                        )
                        .into_any_element()
                    },
                    |viewer| viewer.clone().into_any_element(),
                ),
            })
            .into_any_element();

        window_chrome::decorate(content, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clankerdiff_core::{Review, ReviewSubmission};

    #[test]
    fn maps_host_events_without_opening_a_window() {
        assert_eq!(
            host_event_effect(&DiffReviewEvent::CopyFormattedReview("copy me".into())),
            HostEventEffect::Copy("copy me".into())
        );
        assert_eq!(
            host_event_effect(&DiffReviewEvent::Cancel),
            HostEventEffect::Quit
        );
        let submission = ReviewSubmission {
            comments: Vec::new(),
            formatted: Review::default().submission().formatted,
        };
        let HostEventEffect::PrintSubmission(json) =
            host_event_effect(&DiffReviewEvent::SubmitReview(submission))
        else {
            panic!("submission should be printed");
        };
        assert!(json.contains("formatted"));
    }
}
