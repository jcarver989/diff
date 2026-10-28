use crate::{WINDOW_TITLE, preferences::ThemeStore};
use clankerdiff_core::ReviewCapabilities;
use clankerdiff_gpui::{
    MarkdownReviewer, MarkdownReviewerOptions, ThemeChanged,
    ui::{
        InterfaceSize,
        prelude::{UI_FONT, UiTheme, window_appearance},
        window_chrome,
    },
};
use clankerdiff_gpui_shell::run_loop;
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewEvent, MarkdownReviewSubmission};
use gpui::{AppContext, ClipboardItem, Context, Entity, Subscription, Window, div, prelude::*};
use std::sync::{Arc, mpsc::Sender};

/// Desktop root dedicated to rendered Markdown review.
pub(crate) struct MarkdownDesktopApp {
    reviewer: Entity<MarkdownReviewer>,
    _subscriptions: [Subscription; 3],
}

impl MarkdownDesktopApp {
    pub(crate) fn new(
        document: Arc<MarkdownDocument>,
        outcome: Sender<Option<MarkdownReviewSubmission>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = ThemeStore::user().theme(window_appearance(window));
        let reviewer = cx.new(|cx| {
            let mut reviewer = MarkdownReviewer::with_options(document, theme, MarkdownReviewerOptions::default());
            reviewer
                .set_capabilities(ReviewCapabilities { submit: true, clipboard: true, ..ReviewCapabilities::NONE }, cx);
            reviewer
        });
        let subscription =
            cx.subscribe(&reviewer, move |_this, _reviewer, event: &MarkdownReviewEvent, cx| match event {
                MarkdownReviewEvent::Submit(submission) => {
                    let _ = outcome.send(Some(submission.clone()));
                    run_loop::stop(cx);
                }
                MarkdownReviewEvent::CopyFormatted(text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                }
                MarkdownReviewEvent::Cancel => {
                    let _ = outcome.send(None);
                    run_loop::stop(cx);
                }
            });
        let theme_subscription = cx.subscribe_in(&reviewer, window, |_, _, event: &ThemeChanged, window, _| {
            let _ = ThemeStore::user().remember(window_appearance(window), &event.id);
        });
        let appearance_subscription = cx.observe_window_appearance(window, |this, window, cx| {
            let theme = ThemeStore::user().theme(window_appearance(window));
            this.reviewer.update(cx, |reviewer, cx| reviewer.set_theme(theme, cx));
        });
        Self { reviewer, _subscriptions: [subscription, theme_subscription, appearance_subscription] }
    }
}

impl Render for MarkdownDesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        InterfaceSize::apply(window, cx);
        let content = div().size_full().font_family(UI_FONT).child(self.reviewer.clone()).into_any_element();
        let theme = UiTheme::new(self.reviewer.read(cx).theme());
        window_chrome::decorate(content, WINDOW_TITLE, &theme, window, cx)
    }
}
