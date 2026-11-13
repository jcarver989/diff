use crate::preferences::Preferences;
use clankerdiff_core::ReviewCapabilities;
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewEvent, MarkdownReviewSubmission};
use clankerdiff_rouge::{MarkdownController, MarkdownOutput, markdown_review_view};
use rouge::{
    core::{Styled, View, div, window_handle},
    reactive::effect,
};
use std::{
    cell::RefCell,
    sync::{Arc, mpsc::Sender},
};

pub(crate) fn root(
    document: MarkdownDocument,
    outcome: Sender<Option<MarkdownReviewSubmission>>,
    preferences: Preferences,
) -> impl View {
    let size = preferences.interface_size();
    let initial_theme = preferences.theme();
    let prefs = preferences.clone();
    let window = window_handle();
    let outcome = RefCell::new(Some(outcome));
    let review = MarkdownController::new(Arc::new(document), initial_theme).mount(move |event| match event {
        MarkdownOutput::Event(MarkdownReviewEvent::Submit(submission)) => {
            if let Some(outcome) = outcome.borrow_mut().take() {
                let _ = outcome.send(Some(submission));
            }
            window.quit();
        }
        MarkdownOutput::Event(MarkdownReviewEvent::Cancel) => window.quit(),
        MarkdownOutput::Event(MarkdownReviewEvent::CopyFormatted(text)) | MarkdownOutput::CopyCode(text) => {
            window.copy_text(text);
        }
        MarkdownOutput::ThemeChanged(id) => prefs.remember_theme(&id),
        MarkdownOutput::OpenLink(url) => {
            let _ = rouge::platform::open_url(&url);
        }
    });
    review.set_capabilities(ReviewCapabilities { submit: true, clipboard: true, ..ReviewCapabilities::NONE });
    let theme = review;
    effect(move || theme.set_theme(preferences.theme()));
    size.bind(div().size_full().child(markdown_review_view(review)))
}
