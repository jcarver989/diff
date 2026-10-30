use super::DiffViewer;
use crate::{
    comment_editor::CommentEditor,
    ui::{
        prelude::{SearchPicker, SearchPickerEvent, ThemePickerOverlay, rem},
        tokens,
    },
};
use clankerdiff_core::{DiffReviewCommand, FileStatus, InteractionPhase, RepoPath, ReviewCommand};
use gpui::{Context, Entity, MouseButton, Subscription, Window, div, prelude::*};

pub(crate) enum Overlay {
    Commands(PickerOverlay),
    Commits(PickerOverlay),
    Themes(ThemePickerOverlay),
    Repository(RepositoryPrompt),
}

pub(crate) enum RepositoryPrompt {
    Commit { editor: Entity<CommentEditor>, _subscription: Subscription },
    Discard { path: RepoPath, status: FileStatus },
}

impl Overlay {
    pub(crate) const fn phase(&self) -> InteractionPhase {
        match self {
            Self::Commands(_) => InteractionPhase::Help,
            Self::Commits(_) => InteractionPhase::CommitPicker,
            Self::Themes(_) => InteractionPhase::ThemePicker,
            Self::Repository(_) => InteractionPhase::RepositoryPrompt,
        }
    }

    pub(crate) const fn mode(&self) -> &'static str {
        match self {
            Self::Commands(_) => "commands",
            Self::Commits(_) => "commits",
            Self::Themes(_) => "themes",
            Self::Repository(RepositoryPrompt::Commit { .. }) => "repository-commit",
            Self::Repository(RepositoryPrompt::Discard { .. }) => "repository-discard",
        }
    }
}

pub(crate) struct PickerOverlay {
    pub(crate) picker: Entity<SearchPicker>,
    _subscription: Subscription,
}

impl PickerOverlay {
    pub(crate) fn new(
        picker: Entity<SearchPicker>,
        window: &mut Window,
        cx: &mut Context<DiffViewer>,
        choose: impl Fn(usize) -> Option<DiffReviewCommand> + 'static,
    ) -> Self {
        let subscription =
            cx.subscribe_in(&picker, window, move |viewer, _, event: &SearchPickerEvent, window, cx| match event {
                SearchPickerEvent::Chose(index) => {
                    viewer.overlay = None;
                    if let Some(command) = choose(*index) {
                        viewer.handle_command(command, window, cx);
                    }
                    cx.notify();
                }
                SearchPickerEvent::Dismissed => {
                    viewer.handle_command(ReviewCommand::Cancel, window, cx);
                }
                SearchPickerEvent::Highlighted(_) | SearchPickerEvent::Query(_) => {}
            });
        Self { picker, _subscription: subscription }
    }
}

impl DiffViewer {
    pub(crate) fn close_overlay(&mut self, phase: InteractionPhase) -> Option<Overlay> {
        self.overlay.take_if(|overlay| overlay.phase() == phase)
    }

    pub(crate) fn commit_picker(&self) -> Option<Entity<SearchPicker>> {
        match &self.overlay {
            Some(Overlay::Commits(overlay)) => Some(overlay.picker.clone()),
            _ => None,
        }
    }

    pub(crate) fn render_picker_overlay(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (id, picker) = match &self.overlay {
            Some(Overlay::Commands(overlay)) => ("command-palette-backdrop", Some(overlay.picker.clone())),
            Some(Overlay::Commits(_)) => ("commit-picker-backdrop", None),
            _ => return None,
        };
        Some(
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|viewer, _, window, cx| {
                        cx.stop_propagation();
                        viewer.handle_command(ReviewCommand::Cancel, window, cx);
                    }),
                )
                .children(picker.map(|picker| {
                    div()
                        .absolute()
                        .top(rem(tokens::BAR_HEIGHT))
                        .left_0()
                        .right_0()
                        .px_3()
                        .flex()
                        .justify_center()
                        .child(picker)
                })),
        )
    }
}
