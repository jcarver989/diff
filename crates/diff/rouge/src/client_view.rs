use crate::{DiffViewerOptions, ReviewController, ReviewHandle, ReviewOutput, diff_view};
use clankerdiff_client::{ClientState, ConnectionState};
use clankerdiff_core::{DiffDocument, DiffReviewCommand, LookupReply, RepoPath};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Styled, Theme, View, div, dynamic, show, view};
use rouge_reactive::{Signal, Stored, batch, signal, stored};
use rouge_widgets::{IconName, empty_state};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct DiffClientView {
    review: ReviewHandle,
    state: Signal<Option<(bool, String)>>,
    snapshot: Stored<Option<Arc<DiffDocument>>>,
    files: Stored<Option<Arc<[RepoPath]>>>,
}

impl DiffClientView {
    #[must_use]
    pub fn mount(theme: ReviewTheme, options: DiffViewerOptions, output: impl Fn(ReviewOutput) + 'static) -> Self {
        Self {
            review: ReviewController::with_options(Arc::new(DiffDocument::default()), theme, options).mount(output),
            state: signal(Some((false, String::new()))),
            snapshot: stored(None),
            files: stored(None),
        }
    }

    pub fn set_state(&self, state: &ClientState) {
        if !self.review.is_alive() {
            return;
        }
        let snapshot = self.snapshot.update(|cache| state.snapshot_if_changed(cache));
        let files = self.files.update(|cache| state.files_if_changed(cache));
        batch(|| {
            if let Some(snapshot) = snapshot {
                self.review.set_document(snapshot.clone());
            }
            if let Some(files) = files {
                self.review.set_repository_files(files);
            }
            self.review.set_capabilities(state.capabilities);
            let error = state.status();
            self.state.set_if_changed(
                state.snapshot.is_none().then(|| {
                    (matches!(state.connection, ConnectionState::Failed(_)), error.clone().unwrap_or_default())
                }),
            );
            self.review.set_background_error(error);
        });
    }

    pub fn set_failure(&self, message: String) {
        self.state.try_update(|state| *state = Some((true, message)));
    }

    pub fn command_finished(&self, result: Result<(), String>) {
        self.review.command_finished(result);
    }

    pub fn lookup_finished(&self, reply: LookupReply) {
        self.review.lookup_finished(reply);
    }

    #[must_use]
    pub fn handle_command(&self, command: DiffReviewCommand) -> bool {
        self.state.is_alive() && self.state.get_untracked().is_none() && self.review.handle_command(command)
    }

    #[must_use]
    pub fn review(&self) -> &ReviewHandle {
        &self.review
    }

    #[must_use]
    pub fn view(&self) -> impl View + use<> {
        let state = self.state;
        let handle = self.review;
        let theme = handle.theme();
        view! {
            <div flex col size_full>
                <show when={move || state.get().is_none()}>{diff_view(handle)}</show>
                <dynamic>{
                    state.get().map(|(failed, detail)| {
                        Theme::provide(theme.get().ui);
                        let title = if failed { "Could not load changes" } else { "Loading changes…" };
                        view! { <empty_state icon=IconName::FileDiff title detail/> }
                    })
                }</dynamic>
            </div>
        }
    }
}
