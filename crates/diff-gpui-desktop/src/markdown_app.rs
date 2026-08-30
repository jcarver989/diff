use crate::{preferences, window_chrome};
use diff_gpui::{DEFAULT_FONT_FAMILY, MarkdownReviewer, MarkdownReviewerOptions, ThemeChanged};
use diff_markdown::{MarkdownDocument, MarkdownReviewEvent, MarkdownReviewSubmission};
use gpui::{AppContext, ClipboardItem, Context, Entity, Subscription, Window, div, prelude::*};
use std::sync::{Arc, mpsc::Sender};

/// Desktop root dedicated to rendered Markdown review.
pub(crate) struct MarkdownDesktopApp {
    reviewer: Entity<MarkdownReviewer>,
    _subscription: Subscription,
    _theme_subscription: Subscription,
}

impl MarkdownDesktopApp {
    pub(crate) fn new(
        document: Arc<MarkdownDocument>,
        outcome: Sender<Option<MarkdownReviewSubmission>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = preferences::load_theme();
        let reviewer = cx.new(|_| {
            MarkdownReviewer::with_options(document, theme, MarkdownReviewerOptions::default())
        });
        let subscription = cx.subscribe(
            &reviewer,
            move |_this, _reviewer, event: &MarkdownReviewEvent, cx| match event {
                MarkdownReviewEvent::Submit(submission) => {
                    let _ = outcome.send(Some(submission.clone()));
                    cx.quit();
                }
                MarkdownReviewEvent::CopyFormatted(text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                }
                MarkdownReviewEvent::Cancel => {
                    let _ = outcome.send(None);
                    cx.quit();
                }
            },
        );
        let theme_subscription =
            cx.subscribe(&reviewer, |_this, _reviewer, event: &ThemeChanged, _cx| {
                let _ = preferences::save_theme(&event.id);
            });
        Self {
            reviewer,
            _subscription: subscription,
            _theme_subscription: theme_subscription,
        }
    }
}

impl Render for MarkdownDesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = div()
            .size_full()
            .font_family(DEFAULT_FONT_FAMILY)
            .child(self.reviewer.clone())
            .into_any_element();
        window_chrome::decorate(content, window, cx)
    }
}
