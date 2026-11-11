use crate::{DiffViewerOptions, ReviewController, ReviewHandle, ReviewOutput, diff_view};
use clankerdiff_client::{ClientState, ConnectionState};
use clankerdiff_core::{DiffDocument, DiffReviewCommand, LookupReply, RepoPath};
use clankerdiff_rouge_ui::prelude::{EmptyState, IconName, UiTheme};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Styled, View, div, dynamic, show};
use rouge_reactive::{Signal, batch, signal};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone)]
pub struct DiffClientView {
    review: ReviewHandle,
    state: Signal<Option<(bool, String)>>,
    snapshot: Rc<RefCell<Option<Arc<DiffDocument>>>>,
    files: Rc<RefCell<Option<Arc<[RepoPath]>>>>,
}

impl DiffClientView {
    #[must_use]
    pub fn mount(theme: ReviewTheme, options: DiffViewerOptions, output: impl Fn(ReviewOutput) + 'static) -> Self {
        Self {
            review: ReviewController::with_options(Arc::new(DiffDocument::default()), theme, options).mount(output),
            state: signal(Some((false, String::new()))),
            snapshot: Rc::default(),
            files: Rc::default(),
        }
    }

    pub fn set_state(&self, state: &ClientState) {
        if !self.review.is_alive() {
            return;
        }
        let snapshot = state.snapshot_if_changed(&mut self.snapshot.borrow_mut());
        let files = state.files_if_changed(&mut self.files.borrow_mut());
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
        let handle = self.review.clone();
        let theme = handle.theme();
        div()
            .flex()
            .col()
            .size_full()
            .child(show(move || state.get().is_none(), move || diff_view(handle.clone())))
            .child(dynamic(move || {
                state.get().map_or_else(
                    || div().flex().col().into_any(),
                    |(failed, detail)| {
                        let title = if failed { "Could not load changes" } else { "Loading changes…" };
                        EmptyState::new(IconName::FileDiff, title, detail, UiTheme::new(&theme.get())).into_any()
                    },
                )
            }))
    }
}
