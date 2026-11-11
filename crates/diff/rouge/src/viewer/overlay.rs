use super::{ReviewController, ReviewHandle, ReviewOutput};
use crate::{commits::commit_picker, palette::help_picker};
use clankerdiff_core::{DiffReviewCommand, FileStatus, InteractionPhase, RepoPath, RepositoryAction, ReviewCommand};
use clankerdiff_rouge_ui::{
    components::{ButtonVariant, action_bar, backdrop, button, modal},
    picker::{SearchPickerHandle, ThemePickerEvent, theme_picker, theme_picker_overlay},
};
use clankerdiff_ui_core::ThemeChoice;
use rouge_core::{AnyView, Key, Modifiers, View, div, editable, text, view};
use rouge_reactive::{batch, on_cleanup, signal};
use std::{cell::Cell, rc::Rc};

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
    Themes,
    Commit,
    Discard { path: RepoPath, status: FileStatus },
}

impl Overlay {
    pub(super) const fn phase(&self) -> InteractionPhase {
        match self {
            Self::Help => InteractionPhase::Help,
            Self::Commits => InteractionPhase::CommitPicker,
            Self::Themes => InteractionPhase::ThemePicker,
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
            Some(Overlay::Themes) => OverlayKind::Themes,
            Some(Overlay::Commit) => OverlayKind::Commit,
            Some(Overlay::Discard { .. }) => OverlayKind::Discard,
            None if self.session.draft().is_some() => OverlayKind::Draft,
            None => OverlayKind::None,
        }
    }

    pub(super) fn cancel_overlay(&mut self) -> bool {
        self.overlay.take().is_some()
    }
}

impl ReviewHandle {
    pub(crate) fn dismiss_picker(&self, phase: InteractionPhase) {
        if self.is_alive() && self.status().get_untracked().context.phase == phase {
            self.handle_command(ReviewCommand::Cancel);
        }
    }

    pub(crate) fn choose_palette_command(&self, command: DiffReviewCommand) {
        batch(|| {
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

pub(super) fn view(handle: &ReviewHandle, kind: OverlayKind) -> AnyView {
    let handle = *handle;
    let cancel = move || {
        handle.handle_command(ReviewCommand::Cancel);
    };
    match kind {
        OverlayKind::None | OverlayKind::Draft => ().into_any(),
        OverlayKind::Help => {
            let (picker, palette) = help_picker(&handle);
            handle.picker.set(Some(picker));
            view! { <backdrop id="command-palette-backdrop" align_top on_dismiss=cancel>{palette}</backdrop> }
                .into_any()
        }
        OverlayKind::Commits => {
            let (picker, commits) = commit_picker(&handle);
            handle.picker.set(Some(picker));
            view! {
                <backdrop id="commit-picker-backdrop" anchor_below=handle.commit_anchor() on_dismiss=cancel>
                    {commits}
                </backdrop>
            }
            .into_any()
        }
        OverlayKind::Themes => {
            let original = handle.theme.get_untracked();
            let committed = Rc::new(Cell::new(false));
            on_cleanup({
                let (committed, original) = (committed.clone(), original.clone());
                move || {
                    if !committed.get() {
                        handle.set_theme(original);
                    }
                }
            });
            let picker = theme_picker(&original, ThemeChoice::catalog(), move |event| match event {
                ThemePickerEvent::Preview(theme) => handle.set_theme(theme),
                ThemePickerEvent::Commit(theme) => {
                    committed.set(true);
                    handle.output.call(ReviewOutput::ThemeChanged(theme.id().to_string()));
                    handle.set_theme(theme);
                    handle.dismiss_picker(InteractionPhase::ThemePicker);
                }
                ThemePickerEvent::Cancel(_) => handle.dismiss_picker(InteractionPhase::ThemePicker),
            });
            handle.picker.set(Some(picker));
            view! { <theme_picker_overlay id="theme-picker" handle=picker/> }.into_any()
        }
        OverlayKind::Commit => {
            let value = signal(String::new());
            let status = handle.status();
            view! {
                <modal id="commit-prompt" title="Commit staged changes" on_dismiss=cancel>
                    <editable value test_id="commit-message" autofocus multiline min_rows=3 max_rows=12
                              submit_on_enter=true
                              on_submit={move |body| {
                                  handle.confirm_repository(Some(body));
                              }}/>
                    <text>{move || status.get().repository_error.unwrap_or_default()}</text>
                    <button id="commit-confirm" label="Commit"
                            on_click={move || {
                                handle.confirm_repository(Some(&value.get_untracked()));
                            }}/>
                </modal>
            }
            .into_any()
        }
        OverlayKind::Discard => discard_prompt(&handle).into_any(),
    }
}

fn discard_prompt(handle: &ReviewHandle) -> impl View + use<> {
    let handle = *handle;
    let cancel = move || {
        handle.handle_command(ReviewCommand::Cancel);
    };
    let path = handle.with(|state| match &state.overlay {
        Some(Overlay::Discard { path, .. }) => path.to_string(),
        _ => String::new(),
    });
    let confirm = move || {
        handle.confirm_repository(None);
    };
    view! {
        <modal id="discard-prompt" title="Discard changes?" on_dismiss=cancel>
            <div focusable autofocus tab_index=-1
                 on_key_down={move |event| {
                     if event.modifiers != Modifiers::NONE {
                         return;
                     }
                     match &event.key {
                         Key::Character(key) if key == "y" => {
                             event.stop_propagation();
                             confirm();
                         }
                         Key::Character(key) if key == "n" => {
                             event.stop_propagation();
                             cancel();
                         }
                         _ => (),
                     }
                 }}>
                <text>{format!("Discard changes to {path}?")}</text>
                <action_bar>
                    <button id="discard-cancel" label="Cancel" keyboard_focusable on_click=cancel/>
                    <button id="discard-confirm" label="Discard" variant=ButtonVariant::Destructive
                            keyboard_focusable on_click=confirm/>
                </action_bar>
            </div>
        </modal>
    }
}
