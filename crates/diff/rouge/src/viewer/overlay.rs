use super::{ReviewController, ReviewHandle, ReviewOutput};
use crate::{commits::commit_picker, palette::help_picker};
use clankerdiff_core::{DiffReviewCommand, FileStatus, InteractionPhase, RepoPath, RepositoryAction, ReviewCommand};
use clankerdiff_rouge_ui::{
    components::{ActionBar, Backdrop, Button, ButtonVariant, Modal},
    picker::{SearchPickerHandle, ThemePicker, ThemePickerEvent, ThemePickerOverlay},
    theme::UiTheme,
};
use clankerdiff_ui_core::{ReviewTheme, ThemeChoice};
use rouge_core::{AnyView, Key, Modifiers, View, div, editable, text};
use rouge_reactive::signal;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OverlayKind {
    None,
    Draft,
    Help,
    Commits,
    Themes,
    Commit,
    Discard,
}

pub(super) enum Overlay {
    Help,
    Commits,
    Themes(Box<ReviewTheme>),
    Commit,
    Discard { path: RepoPath, status: FileStatus },
}

impl Overlay {
    pub(super) const fn phase(&self) -> InteractionPhase {
        match self {
            Self::Help => InteractionPhase::Help,
            Self::Commits => InteractionPhase::CommitPicker,
            Self::Themes(_) => InteractionPhase::ThemePicker,
            Self::Commit | Self::Discard { .. } => InteractionPhase::RepositoryPrompt,
        }
    }
}

pub(super) enum PickerCommand {
    Highlight(usize),
    Move(isize),
    Choose,
}

impl PickerCommand {
    pub(super) fn apply(self, picker: &SearchPickerHandle) {
        if !picker.query().is_alive() {
            return;
        }
        match self {
            Self::Highlight(index) => picker.highlight(index),
            Self::Move(delta) => picker.move_highlight(delta),
            Self::Choose => picker.choose(),
        }
    }
}

impl ReviewController {
    pub(super) const fn overlay_kind(&self) -> OverlayKind {
        match &self.overlay {
            Some(Overlay::Help) => OverlayKind::Help,
            Some(Overlay::Commits) => OverlayKind::Commits,
            Some(Overlay::Themes(_)) => OverlayKind::Themes,
            Some(Overlay::Commit) => OverlayKind::Commit,
            Some(Overlay::Discard { .. }) => OverlayKind::Discard,
            None if self.session.draft().is_some() => OverlayKind::Draft,
            None => OverlayKind::None,
        }
    }

    pub(super) fn cancel_overlay(&mut self) -> bool {
        match self.overlay.take() {
            Some(Overlay::Themes(original)) => {
                self.theme = *original;
                true
            }
            Some(_) => true,
            None => false,
        }
    }
}

impl ReviewHandle {
    pub(crate) fn dismiss_picker(&self, phase: InteractionPhase) {
        if self.is_alive() && self.status().get_untracked().context.phase == phase {
            self.handle_command(ReviewCommand::Cancel);
        }
    }

    pub(crate) fn choose_palette_command(&self, command: DiffReviewCommand) {
        rouge_reactive::batch(|| {
            let closed = self.update(|state| {
                if matches!(state.overlay, Some(Overlay::Help)) {
                    state.overlay = None;
                    true
                } else {
                    false
                }
            });
            if closed == Some(true) {
                self.handle_command(command);
            }
        });
    }

    pub(super) fn confirm_repository(&self, message: Option<&str>) -> bool {
        self.update(|state| {
            let action = match (&state.overlay, message) {
                (Some(Overlay::Commit), Some(message)) => {
                    let message = message.trim();
                    if message.is_empty() {
                        state.repository_error = Some("Commit message cannot be empty".to_owned());
                        return true;
                    }
                    RepositoryAction::Commit { message: message.to_owned() }
                }
                (Some(Overlay::Discard { path, status }), None) => {
                    RepositoryAction::Discard { path: path.clone(), status: *status }
                }
                _ => return false,
            };
            state.overlay = None;
            state.handle_command(DiffReviewCommand::RepositoryAction(action))
        })
        .unwrap_or(false)
    }
}

#[expect(clippy::too_many_lines, reason = "Keep the overlay view dispatch exhaustive")]
pub(super) fn view(handle: &ReviewHandle, kind: OverlayKind, theme: UiTheme) -> AnyView {
    let cancel = {
        let handle = handle.clone();
        move || {
            handle.handle_command(ReviewCommand::Cancel);
        }
    };
    match kind {
        OverlayKind::None | OverlayKind::Draft => div().into_any(),
        OverlayKind::Help | OverlayKind::Commits => {
            let picker =
                if kind == OverlayKind::Help { help_picker(handle, theme) } else { commit_picker(handle, theme) };
            handle.picker.set(Some(picker.handle()));
            let backdrop = if kind == OverlayKind::Help {
                Backdrop::new("command-palette-backdrop", theme).align_top()
            } else {
                Backdrop::new("commit-picker-backdrop", theme).anchor_below(handle.commit_anchor())
            };
            backdrop.on_dismiss(cancel).child(picker).into_any()
        }
        OverlayKind::Themes => {
            let current = handle.theme.get_untracked();
            let picker = ThemePicker::new("theme-picker", &current, ThemeChoice::catalog(), theme, {
                let handle = handle.clone();
                move |event| {
                    handle.update(|state| {
                        if !matches!(state.overlay, Some(Overlay::Themes(_))) {
                            return;
                        }
                        match event {
                            ThemePickerEvent::Preview(theme) => state.theme = theme,
                            ThemePickerEvent::Commit(theme) => {
                                state.overlay = None;
                                state.outputs.push(ReviewOutput::ThemeChanged(theme.id().to_string()));
                                state.theme = theme;
                            }
                            ThemePickerEvent::Cancel(_) => {
                                state.cancel_overlay();
                            }
                        }
                    });
                }
            });
            handle.picker.set(Some(picker.handle()));
            ThemePickerOverlay::new(picker, theme).into_any()
        }
        OverlayKind::Commit => {
            let value = signal(String::new());
            let status = handle.status();
            Modal::new("commit-prompt", "Commit staged changes", theme)
                .on_dismiss(cancel)
                .child(
                    editable(value)
                        .test_id("commit-message")
                        .autofocus()
                        .multiline()
                        .min_rows(3)
                        .max_rows(12)
                        .submit_on_enter(true)
                        .on_submit({
                            let handle = handle.clone();
                            move |body| {
                                handle.confirm_repository(Some(body));
                            }
                        }),
                )
                .child(text(move || status.get().repository_error.unwrap_or_default()))
                .child(Button::new("commit-confirm", "Commit", theme).on_click({
                    let handle = handle.clone();
                    move || {
                        handle.confirm_repository(Some(&value.get_untracked()));
                    }
                }))
                .into_any()
        }
        OverlayKind::Discard => {
            let path = handle.with(|state| match &state.overlay {
                Some(Overlay::Discard { path, .. }) => path.to_string(),
                _ => String::new(),
            });
            Modal::new("discard-prompt", "Discard changes?", theme)
                .on_dismiss(cancel.clone())
                .child(
                    div()
                        .focusable()
                        .autofocus()
                        .tab_index(-1)
                        .on_key_down({
                            let handle = handle.clone();
                            let cancel = cancel.clone();
                            move |event| {
                                if event.modifiers != Modifiers::NONE {
                                    return;
                                }
                                match &event.key {
                                    Key::Character(key) if key == "y" => {
                                        event.stop_propagation();
                                        handle.confirm_repository(None);
                                    }
                                    Key::Character(key) if key == "n" => {
                                        event.stop_propagation();
                                        cancel();
                                    }
                                    _ => (),
                                }
                            }
                        })
                        .child(text(format!("Discard changes to {path}?")))
                        .child(
                            ActionBar::new(theme)
                                .child(
                                    Button::new("discard-cancel", "Cancel", theme)
                                        .keyboard_focusable()
                                        .on_click(cancel),
                                )
                                .child(
                                    Button::new("discard-confirm", "Discard", theme)
                                        .variant(ButtonVariant::Destructive)
                                        .keyboard_focusable()
                                        .on_click({
                                            let handle = handle.clone();
                                            move || {
                                                handle.confirm_repository(None);
                                            }
                                        }),
                                ),
                        ),
                )
                .into_any()
        }
    }
}
