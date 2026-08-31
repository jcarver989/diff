#![allow(missing_docs)] // GPUI action declarations cannot carry per-action documentation.

use crate::{args::CliArgs, preferences, window_chrome};
use diff_core::{DiffReviewEvent, DiffScope, RepositoryAction, ReviewSubmission};
use diff_git::{GitError, GitRepository, RepositorySnapshot};
use diff_gpui::{
    DEFAULT_FONT_FAMILY, DiffViewer, DiffViewerOptions, ThemeChanged, default_font_size,
    ui::prelude::{EmptyState, NoticeTone, UiTheme},
};
use diff_theme::DiffTheme;
use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, KeyBinding, Subscription, Task, Window,
    actions, div, prelude::*,
};
use std::{future::Future, path::PathBuf, sync::mpsc::Sender};

type LoadResult = Result<(Option<GitRepository>, RepositorySnapshot), GitError>;

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

/// Builds the viewer options used when a repository snapshot is installed,
/// deriving the initial font size from the window's pixel density.
fn viewer_options(scale_factor: f32) -> DiffViewerOptions {
    DiffViewerOptions {
        font_size: default_font_size(scale_factor),
        ..DiffViewerOptions::default()
    }
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
    repository_path: PathBuf,
    repository: Option<GitRepository>,
    scope: DiffScope,
    state: LoadState,
    viewer: Option<Entity<DiffViewer>>,
    viewer_subscription: Option<Subscription>,
    theme_subscription: Option<Subscription>,
    theme: DiffTheme,
    load_task: Task<()>,
    outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
    scale_factor: f32,
}

impl DesktopApp {
    pub(crate) fn new(
        args: CliArgs,
        outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scale_factor = window.scale_factor();
        let mut app = Self {
            repository_path: args.repository,
            repository: None,
            scope: args.scope,
            state: LoadState::Loading,
            viewer: None,
            viewer_subscription: None,
            theme_subscription: None,
            theme: preferences::load_theme(),
            load_task: Task::ready(()),
            outcome_sender,
            scale_factor,
        };
        app.discover(cx);
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

    fn load<F>(&mut self, cx: &mut Context<Self>, work: F)
    where
        F: Future<Output = LoadResult> + Send + 'static,
    {
        self.state = LoadState::Loading;
        let operation = gpui_tokio::Tokio::spawn(cx, work);
        self.load_task = cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok((repository, snapshot))) => {
                    if let Some(repository) = repository {
                        this.repository = Some(repository);
                    }
                    this.install_snapshot(&snapshot, cx);
                }
                Ok(Err(error)) => this.set_error(&error, cx),
                Err(error) => {
                    this.state = LoadState::Error(format!("background task failed: {error}"));
                    cx.notify();
                }
            });
        });
        cx.notify();
    }

    fn discover(&mut self, cx: &mut Context<Self>) {
        let path = self.repository_path.clone();
        let scope = self.scope;
        self.load(cx, async move {
            let repository = GitRepository::discover(path).await?;
            let snapshot = repository.snapshot_with_sources(scope).await?;
            Ok((Some(repository), snapshot))
        });
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            self.discover(cx);
            return;
        };
        let scope = self.scope;
        self.load(cx, async move {
            repository
                .snapshot_with_sources(scope)
                .await
                .map(|snapshot| (None, snapshot))
        });
    }

    fn mutate_all(&mut self, stage: bool, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let scope = self.scope;
        self.load(cx, async move {
            if stage {
                repository.stage_all().await?;
            } else {
                repository.unstage_all().await?;
            }
            repository
                .snapshot_with_sources(scope)
                .await
                .map(|snapshot| (None, snapshot))
        });
    }

    fn mutate(&mut self, action: RepositoryAction, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| viewer.set_repository_pending(true, cx));
        }
        let scope = self.scope;
        let operation = gpui_tokio::Tokio::spawn(cx, async move {
            match action {
                RepositoryAction::StagePaths(paths) => repository.stage(&paths).await?,
                RepositoryAction::UnstagePaths(paths) => repository.unstage(&paths).await?,
                RepositoryAction::StageAll => repository.stage_all().await?,
                RepositoryAction::UnstageAll => repository.unstage_all().await?,
                RepositoryAction::Commit { message } => repository.commit(&message).await?,
                RepositoryAction::Discard { path, status } => {
                    repository.discard(&path, status).await?;
                }
                RepositoryAction::Refresh => {}
            }
            repository.snapshot_with_sources(scope).await
        });
        self.load_task = cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(snapshot)) => this.install_snapshot(&snapshot, cx),
                Ok(Err(error)) => {
                    if let Some(viewer) = &this.viewer {
                        viewer.update(cx, |viewer, cx| {
                            viewer.set_repository_error(error.to_string(), cx);
                        });
                    }
                }
                Err(error) => {
                    if let Some(viewer) = &this.viewer {
                        viewer.update(cx, |viewer, cx| {
                            viewer.set_repository_error(
                                format!("background task failed: {error}"),
                                cx,
                            );
                        });
                    }
                }
            });
        });
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
            let options = viewer_options(self.scale_factor);
            let viewer = cx.new(|_| {
                DiffViewer::from_snapshot_with_options(diff_snapshot.clone(), theme, options)
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

    fn set_error(&mut self, error: &GitError, cx: &mut Context<Self>) {
        self.state = LoadState::Error(error.to_string());
        cx.notify();
    }

    fn handle_viewer_event(&mut self, event: &DiffReviewEvent, cx: &mut Context<Self>) {
        if let DiffReviewEvent::RepositoryAction(action) = event {
            self.mutate(action.clone(), cx);
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

    fn refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn cycle_scope(&mut self, _: &CycleScope, _: &mut Window, cx: &mut Context<Self>) {
        self.scope = self.scope.next();
        self.reload(cx);
    }

    fn stage_all(&mut self, _: &StageAll, _: &mut Window, cx: &mut Context<Self>) {
        self.mutate_all(true, cx);
    }

    fn unstage_all(&mut self, _: &UnstageAll, _: &mut Window, cx: &mut Context<Self>) {
        self.mutate_all(false, cx);
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
    fn derives_initial_font_size_from_pixel_density() {
        assert!((viewer_options(1.0).font_size - 16.0).abs() < f32::EPSILON);
        assert!((viewer_options(2.0).font_size - 10.0).abs() < f32::EPSILON);
        // Options other than the derived font size keep their defaults.
        let defaults = DiffViewerOptions::default();
        assert!((viewer_options(1.0).row_height - defaults.row_height).abs() < f32::EPSILON);
    }

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
