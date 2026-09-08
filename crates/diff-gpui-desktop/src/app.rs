#![allow(missing_docs)] // GPUI action declarations cannot carry per-action documentation.

use crate::{args::CliArgs, menus::SetScope, preferences, window_chrome};
use clankerdiff_core::{DiffReviewEvent, DiffScope, RepositoryAction, ReviewSubmission};
use clankerdiff_git::{GitError, GitRepository, RepositorySnapshot};
use clankerdiff_gpui::{
    DEFAULT_FONT_FAMILY, DiffViewer, DiffViewerOptions, ThemeChanged,
    ui::prelude::{EmptyState, NoticeTone, UiTheme},
};
use clankerdiff_theme::DiffTheme;
use clankerdiff_watch::{RepositoryRequest, RepositoryWatcher, WatchError, WatchOptions};
use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, KeyBinding, Subscription, Task, Window,
    actions, div, prelude::*,
};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc::Sender},
};
use tokio::sync::oneshot;

actions!(desktop_diff, [StageAll, UnstageAll]);

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
        DiffReviewEvent::RepositoryAction(_) | DiffReviewEvent::SetScope(_) => {
            HostEventEffect::None
        }
        DiffReviewEvent::CopyFormattedReview(text) => HostEventEffect::Copy(text.clone()),
        DiffReviewEvent::SubmitReview(submission) => HostEventEffect::PrintSubmission(
            serde_json::to_string_pretty(submission)
                .unwrap_or_else(|error| format!("{{\"serialization_error\":\"{error}\"}}")),
        ),
        DiffReviewEvent::Cancel => HostEventEffect::Quit,
    }
}

enum HostCommand {
    Apply(RepositoryAction),
    SetScope(DiffScope),
}

pub(crate) struct DesktopApp {
    repository_path: PathBuf,
    repository: Option<GitRepository>,
    watcher: Option<RepositoryWatcher>,
    scope: DiffScope,
    installed_snapshot: Option<Arc<RepositorySnapshot>>,
    state: LoadState,
    viewer: Option<Entity<DiffViewer>>,
    viewer_subscription: Option<Subscription>,
    theme_subscription: Option<Subscription>,
    theme: DiffTheme,
    load_task: Option<Task<()>>,
    command_task: Option<Task<()>>,
    watch_task: Task<()>,
    outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
}

impl DesktopApp {
    pub(crate) fn new(
        args: CliArgs,
        outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut app = Self {
            repository_path: args.repository,
            repository: None,
            watcher: None,
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
        app.discover(cx);
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
            let repository = GitRepository::discover(path).await?;
            let watcher =
                RepositoryWatcher::spawn(repository.clone(), scope, WatchOptions::default())
                    .await?;
            Ok::<_, WatchError>((repository, watcher))
        });
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| {
                this.load_task = None;
                match result {
                    Ok(Ok((repository, watcher))) => {
                        this.repository = Some(repository);
                        this.install_watcher(watcher, cx);
                    }
                    Ok(Err(error)) => this.set_error(&error.to_string(), cx),
                    Err(error) => this.set_error(&format!("background task failed: {error}"), cx),
                }
            });
        }));
        cx.notify();
    }

    fn install_watcher(&mut self, watcher: RepositoryWatcher, cx: &mut Context<Self>) {
        let mut updates = watcher.snapshot_rx.clone();
        let state = updates.borrow_and_update().clone();
        self.watcher = Some(watcher);
        self.apply_watch_state(&state, cx);
        self.watch_task = cx.spawn(async move |this, cx| {
            while updates.changed().await.is_ok() {
                let state = updates.borrow_and_update().clone();
                if this
                    .update(cx, |this, cx| this.apply_watch_state(&state, cx))
                    .is_err()
                {
                    return;
                }
            }
        });
    }

    fn apply_watch_state(
        &mut self,
        result: &Result<Arc<RepositorySnapshot>, Arc<GitError>>,
        cx: &mut Context<Self>,
    ) {
        let error = match result {
            Ok(snapshot) => {
                if self.installed_snapshot.as_ref() != Some(snapshot) {
                    self.scope = snapshot.scope;
                    self.install_snapshot(snapshot, cx);
                    self.installed_snapshot = Some(Arc::clone(snapshot));
                }
                None
            }
            Err(error) => Some(error.to_string()),
        };
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.set_background_error(error, cx);
            });
        } else if let Some(error) = error {
            self.set_error(&error, cx);
        }
    }

    fn command(&mut self, command: HostCommand, cx: &mut Context<Self>) {
        // A single command owns pending state; snapshots never settle it.
        if self.command_task.is_some() {
            return;
        }
        let Some(watcher) = &self.watcher else {
            return;
        };
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let requests = watcher.request_tx.clone();
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| viewer.set_repository_pending(true, cx));
        }
        let operation = gpui_tokio::Tokio::spawn(cx, async move {
            match command {
                HostCommand::Apply(action) => repository
                    .apply(action)
                    .await
                    .map_err(|error| error.to_string()),
                HostCommand::SetScope(scope) => {
                    let (reply, completion) = oneshot::channel();
                    requests
                        .send(RepositoryRequest::SetScope {
                            scope,
                            result_tx: reply,
                        })
                        .await
                        .map_err(|_| WatchError::Stopped.to_string())?;
                    completion
                        .await
                        .map_err(|_| WatchError::Stopped.to_string())?
                        .map_err(|error| error.to_string())
                }
            }
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

    fn install_snapshot(&mut self, snapshot: &RepositorySnapshot, cx: &mut Context<Self>) {
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
                    if let Ok(theme) = DiffTheme::builtin(&event.id) {
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
            DiffReviewEvent::RepositoryAction(action) => {
                self.command(HostCommand::Apply(action.clone()), cx);
                return;
            }
            DiffReviewEvent::SetScope(scope) => {
                self.set_scope(*scope, cx);
                return;
            }
            _ => {}
        }
        if let Some(sender) = &self.outcome_sender {
            match event {
                DiffReviewEvent::RepositoryAction(_) | DiffReviewEvent::SetScope(_) => {
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

    fn stage_all(&mut self, _: &StageAll, _: &mut Window, cx: &mut Context<Self>) {
        self.command(HostCommand::Apply(RepositoryAction::StageAll), cx);
    }

    fn unstage_all(&mut self, _: &UnstageAll, _: &mut Window, cx: &mut Context<Self>) {
        self.command(HostCommand::Apply(RepositoryAction::UnstageAll), cx);
    }

    fn menu_set_scope(&mut self, action: &SetScope, _: &mut Window, cx: &mut Context<Self>) {
        self.set_scope(action.scope(), cx);
    }

    fn set_scope(&mut self, scope: DiffScope, cx: &mut Context<Self>) {
        self.command(HostCommand::SetScope(scope), cx);
    }

    fn render_empty(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        use clankerdiff_gpui::ui::prelude::{Button, ControlSize};
        let scope = self.scope;
        let pending = self.command_task.is_some();
        let theme = UiTheme::new(&self.theme);
        let segment = |id: &'static str, label: &'static str, value: DiffScope| {
            Button::new(id, label, theme)
                .size(ControlSize::Small)
                .selected(scope == value)
                .disabled(pending)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_scope(value, cx);
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
                NoticeTone::Info,
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
