#![allow(missing_docs)] // GPUI action declarations cannot carry per-action documentation.

use crate::args::CliArgs;
use diff_core::{DiffDocument, DiffReviewEvent, DiffScope};
use diff_git::{GitError, GitRepository};
use diff_gpui::DiffViewer;
use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, KeyBinding, Subscription, Task, Window,
    actions, div, prelude::*,
};
use std::{path::PathBuf, sync::Arc};

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
    Copy(String),
    PrintSubmission(String),
    Quit,
}

fn host_event_effect(event: &DiffReviewEvent) -> HostEventEffect {
    match event {
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
    load_task: Task<()>,
}

impl DesktopApp {
    pub(crate) fn new(args: CliArgs, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut app = Self {
            repository_path: args.repository,
            repository: None,
            scope: args.scope,
            state: LoadState::Loading,
            viewer: None,
            viewer_subscription: None,
            load_task: Task::ready(()),
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

    fn discover(&mut self, cx: &mut Context<Self>) {
        self.state = LoadState::Loading;
        let path = self.repository_path.clone();
        let scope = self.scope;
        let operation = gpui_tokio::Tokio::spawn(cx, async move {
            let repository = GitRepository::discover(path).await?;
            let document = repository.snapshot(scope).await?;
            Ok::<_, GitError>((repository, document))
        });
        self.load_task = cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok((repository, document))) => {
                    this.repository = Some(repository);
                    this.install_document(document, cx);
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

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            self.discover(cx);
            return;
        };
        self.state = LoadState::Loading;
        let scope = self.scope;
        let operation =
            gpui_tokio::Tokio::spawn(cx, async move { repository.snapshot(scope).await });
        self.load_task = cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(document)) => this.install_document(document, cx),
                Ok(Err(error)) => this.set_error(&error, cx),
                Err(error) => {
                    this.state = LoadState::Error(format!("background task failed: {error}"));
                    cx.notify();
                }
            });
        });
        cx.notify();
    }

    fn mutate_all(&mut self, stage: bool, cx: &mut Context<Self>) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        self.state = LoadState::Loading;
        let scope = self.scope;
        let operation = gpui_tokio::Tokio::spawn(cx, async move {
            if stage {
                repository.stage_all().await?;
            } else {
                repository.unstage_all().await?;
            }
            repository.snapshot(scope).await
        });
        self.load_task = cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(document)) => this.install_document(document, cx),
                Ok(Err(error)) => this.set_error(&error, cx),
                Err(error) => {
                    this.state = LoadState::Error(format!("background task failed: {error}"));
                    cx.notify();
                }
            });
        });
        cx.notify();
    }

    fn install_document(&mut self, document: DiffDocument, cx: &mut Context<Self>) {
        let is_empty = document.files.is_empty();
        let document = Arc::new(document);
        if let Some(viewer) = &self.viewer {
            viewer.update(cx, |viewer, cx| viewer.set_document(document, cx));
        } else {
            let viewer = cx.new(|_| DiffViewer::new(document));
            self.viewer_subscription = Some(cx.subscribe(
                &viewer,
                |_this, _viewer, event: &DiffReviewEvent, cx| {
                    Self::handle_viewer_event(event, cx);
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

    fn handle_viewer_event(event: &DiffReviewEvent, cx: &mut Context<Self>) {
        match host_event_effect(event) {
            HostEventEffect::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            HostEventEffect::PrintSubmission(json) => println!("{json}"),
            HostEventEffect::Quit => cx.quit(),
        }
    }

    fn refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn cycle_scope(&mut self, _: &CycleScope, _: &mut Window, cx: &mut Context<Self>) {
        self.scope = match self.scope {
            DiffScope::Unstaged => DiffScope::Staged,
            DiffScope::Staged => DiffScope::Both,
            DiffScope::Both => DiffScope::Unstaged,
        };
        self.reload(cx);
    }

    fn stage_all(&mut self, _: &StageAll, _: &mut Window, cx: &mut Context<Self>) {
        self.mutate_all(true, cx);
    }

    fn unstage_all(&mut self, _: &UnstageAll, _: &mut Window, cx: &mut Context<Self>) {
        self.mutate_all(false, cx);
    }

    fn status_panel(title: &str, detail: &str) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(gpui::rgb(0x0010_1418))
            .text_color(gpui::rgb(0x00d6_d9dc))
            .child(div().text_xl().child(title.to_owned()))
            .child(
                div()
                    .text_sm()
                    .text_color(gpui::rgb(0x008d_969f))
                    .child(detail.to_owned()),
            )
    }
}

impl Render for DesktopApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("DesktopDiff")
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::cycle_scope))
            .on_action(cx.listener(Self::stage_all))
            .on_action(cx.listener(Self::unstage_all))
            .size_full()
            .font_family("Lilex")
            .child(match &self.state {
                LoadState::Loading => {
                    Self::status_panel("Loading diff…", "Git is reading the repository")
                        .into_any_element()
                }
                LoadState::Error(error) => Self::status_panel(
                    "Could not load diff",
                    &format!("{error} · press ⌘/Ctrl+R to retry"),
                )
                .into_any_element(),
                LoadState::Empty => Self::status_panel(
                    "No changes",
                    &format!("Scope: {:?} · press ⇧⌘/Ctrl+V to change scope", self.scope),
                )
                .into_any_element(),
                LoadState::Ready => self.viewer.as_ref().map_or_else(
                    || {
                        Self::status_panel("No viewer", "Press ⌘/Ctrl+R to retry")
                            .into_any_element()
                    },
                    |viewer| viewer.clone().into_any_element(),
                ),
            })
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
