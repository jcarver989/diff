#![allow(missing_docs)] // GPUI action declarations cannot carry per-action documentation.

use crate::{
    args::CliArgs,
    preferences,
    repository_worker::{RepositoryClient, RepositoryUpdate, RepositoryWorker},
    window_chrome,
};
use diff_core::{DiffReviewEvent, DiffScope, RepositoryAction, ReviewSubmission};
use diff_git::RepositorySnapshot;
use diff_gpui::{
    DEFAULT_FONT_FAMILY, DiffViewer, DiffViewerOptions, ThemeChanged,
    ui::prelude::{EmptyState, NoticeTone, UiTheme},
};
use diff_theme::DiffTheme;
use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, KeyBinding, Subscription, Task, Window,
    actions, div, prelude::*,
};
use std::sync::mpsc::Sender;

actions!(desktop_diff, [Refresh, CycleScope, StageAll, UnstageAll]);

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
        DiffReviewEvent::RepositoryAction(_) => HostEventEffect::None,
        DiffReviewEvent::CopyFormattedReview(text) => HostEventEffect::Copy(text.clone()),
        DiffReviewEvent::SubmitReview(submission) => HostEventEffect::PrintSubmission(
            serde_json::to_string_pretty(submission)
                .unwrap_or_else(|error| format!("{{\"serialization_error\":\"{error}\"}}")),
        ),
        DiffReviewEvent::Cancel => HostEventEffect::Quit,
    }
}

pub(crate) struct DesktopApp {
    scope: DiffScope,
    state: LoadState,
    viewer: Option<Entity<DiffViewer>>,
    viewer_subscription: Option<Subscription>,
    theme_subscription: Option<Subscription>,
    theme: DiffTheme,
    repository: RepositoryClient,
    _worker_task: Task<()>,
    repository_update_task: Task<()>,
    outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
}

impl DesktopApp {
    pub(crate) fn new(
        args: CliArgs,
        outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (repository, updates, worker) = RepositoryWorker::new(args.repository, args.scope);
        let worker_operation = gpui_tokio::Tokio::spawn(cx, worker.run());
        let worker_task = cx.spawn(async move |_this, _cx| {
            let _ = worker_operation.await;
        });
        let mut app = Self {
            scope: args.scope,
            state: LoadState::Loading,
            viewer: None,
            viewer_subscription: None,
            theme_subscription: None,
            theme: preferences::load_theme(),
            repository,
            _worker_task: worker_task,
            repository_update_task: Task::ready(()),
            outcome_sender,
        };
        app.repository_update_task = cx.spawn(async move |this, cx| {
            while let Ok(update) = updates.recv().await {
                if this
                    .update(cx, |this, cx| this.apply_repository_update(update, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        app
    }

    pub(crate) fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("cmd-r", Refresh, Some("DesktopDiff")),
            KeyBinding::new("ctrl-r", Refresh, Some("DesktopDiff")),
            KeyBinding::new("cmd-shift-v", CycleScope, Some("DesktopDiff")),
            KeyBinding::new("ctrl-shift-v", CycleScope, Some("DesktopDiff")),
            KeyBinding::new("cmd-shift-s", StageAll, Some("DesktopDiff")),
            KeyBinding::new("ctrl-shift-s", StageAll, Some("DesktopDiff")),
            KeyBinding::new("cmd-shift-u", UnstageAll, Some("DesktopDiff")),
            KeyBinding::new("ctrl-shift-u", UnstageAll, Some("DesktopDiff")),
        ]);
    }

    fn apply_repository_update(&mut self, update: RepositoryUpdate, cx: &mut Context<Self>) {
        match update {
            RepositoryUpdate::MutationPending => {
                if let Some(viewer) = &self.viewer {
                    viewer.update(cx, |viewer, cx| viewer.set_repository_pending(true, cx));
                }
            }
            RepositoryUpdate::Snapshot(snapshot) => self.install_snapshot(&snapshot, cx),
            RepositoryUpdate::Error { message, initial } => {
                if initial || self.viewer.is_none() {
                    self.state = LoadState::Error(message);
                    cx.notify();
                } else if let Some(viewer) = &self.viewer {
                    viewer.update(cx, |viewer, cx| viewer.set_repository_error(message, cx));
                }
            }
            RepositoryUpdate::WatcherWarning(message) => {
                if let Some(viewer) = &self.viewer {
                    viewer.update(cx, |viewer, cx| viewer.set_repository_error(message, cx));
                }
            }
        }
    }

    fn mutate_all(&self, stage: bool) {
        self.repository.mutate(if stage {
            RepositoryAction::StageAll
        } else {
            RepositoryAction::UnstageAll
        });
    }

    fn mutate(&self, action: RepositoryAction) {
        self.repository.mutate(action);
    }

    fn install_snapshot(&mut self, snapshot: &RepositorySnapshot, cx: &mut Context<Self>) {
        let is_empty = snapshot.document.files.is_empty();
        let diff_snapshot = snapshot.diff_snapshot();
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| {
                viewer.set_snapshot(diff_snapshot.clone(), cx);
            });
        } else {
            let theme = self.theme.clone();
            let viewer = cx.new(|_| {
                DiffViewer::from_snapshot_with_options(
                    diff_snapshot.clone(),
                    theme,
                    DiffViewerOptions::default(),
                )
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

    fn handle_viewer_event(&mut self, event: &DiffReviewEvent, cx: &mut Context<Self>) {
        if let DiffReviewEvent::RepositoryAction(action) = event {
            self.mutate(action.clone());
            return;
        }
        if let Some(sender) = &self.outcome_sender {
            match event {
                DiffReviewEvent::RepositoryAction(_) => unreachable!("handled above"),
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

    fn refresh(&mut self, _: &Refresh, _: &mut Window, _: &mut Context<Self>) {
        self.repository.refresh();
    }

    fn cycle_scope(&mut self, _: &CycleScope, _: &mut Window, _: &mut Context<Self>) {
        self.scope = self.scope.next();
        self.repository.set_scope(self.scope);
    }

    fn stage_all(&mut self, _: &StageAll, _: &mut Window, _: &mut Context<Self>) {
        self.mutate_all(true);
    }

    fn unstage_all(&mut self, _: &UnstageAll, _: &mut Window, _: &mut Context<Self>) {
        self.mutate_all(false);
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
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::cycle_scope))
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
                LoadState::Empty => self
                    .status_panel(
                        "No changes",
                        &format!("Scope: {} · press ⇧⌘/Ctrl+V to change scope", self.scope),
                        NoticeTone::Info,
                    )
                    .into_any_element(),
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
    use diff_core::{Review, ReviewSubmission};

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
