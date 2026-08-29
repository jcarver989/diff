//! Native GPUI host for standalone and agent-driven diff review sessions.

mod app;
pub mod args;
mod markdown_app;
mod window_chrome;

use app::DesktopApp;
use args::CliArgs;
use diff_core::{DiffScope, MarkdownDocument, MarkdownReviewSubmission, ReviewSubmission};
use diff_gpui::{DiffViewer, MarkdownReviewer, load_default_fonts};
use gpui::{
    App, AppContext, Bounds, Pixels, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use markdown_app::MarkdownDesktopApp;
use std::{path::PathBuf, sync::mpsc};

fn window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("ClankerDiff".into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Maximized(bounds)),
        window_min_size: Some(size(px(640.0), px(480.0))),
        focus: true,
        is_movable: true,
        is_resizable: true,
        is_minimizable: true,
        window_decorations: None,
        ..Default::default()
    }
}

/// Runs the regular desktop application.
pub fn run(args: CliArgs) {
    run_application(args, None);
}

/// Runs a one-shot desktop review and returns its submitted feedback.
///
/// Closing or cancelling the window returns `None`.
#[must_use]
pub fn run_review(repository: PathBuf, scope: DiffScope) -> Option<ReviewSubmission> {
    let (sender, receiver) = mpsc::channel();
    run_application(CliArgs { repository, scope }, Some(sender));
    receiver.try_recv().ok().flatten()
}

/// Runs a one-shot rendered Markdown review.
///
/// Closing or cancelling the window returns `None`.
///
/// # Panics
/// Panics when bundled fonts cannot be loaded or the native window cannot open.
#[must_use]
pub fn run_markdown_review(document: MarkdownDocument) -> Option<MarkdownReviewSubmission> {
    let (sender, receiver) = mpsc::channel();
    gpui_platform::application().run(move |cx: &mut App| {
        load_default_fonts(cx).expect("failed to load the bundled fonts");
        MarkdownReviewer::bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1280.0), px(840.0)), cx);
        cx.open_window(window_options(bounds), |_window, cx| {
            cx.new(|cx| MarkdownDesktopApp::new(std::sync::Arc::new(document), sender, cx))
        })
        .expect("failed to open the Markdown review window");
        cx.activate(true);
    });
    receiver.try_recv().ok().flatten()
}

fn run_application(args: CliArgs, outcome_sender: Option<mpsc::Sender<Option<ReviewSubmission>>>) {
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_tokio::init(cx);
        load_default_fonts(cx).expect("failed to load the bundled fonts");
        DiffViewer::bind_keys(cx);
        DesktopApp::bind_keys(cx);

        let bounds = Bounds::centered(None, size(px(1280.0), px(840.0)), cx);
        cx.open_window(window_options(bounds), |window, cx| {
            cx.new(|cx| DesktopApp::new(args, outcome_sender, window, cx))
        })
        .expect("failed to open the diff review window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    #[test]
    fn constructs_explicit_native_first_window_options() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(1280.0), px(840.0)));
        let options = window_options(bounds);

        assert_eq!(options.window_bounds, Some(WindowBounds::Maximized(bounds)));
        assert_eq!(options.window_min_size, Some(size(px(640.0), px(480.0))));
        assert!(options.is_movable);
        assert!(options.is_resizable);
        assert!(options.is_minimizable);
        assert_eq!(options.window_decorations, None);
        assert_eq!(
            options.titlebar.and_then(|titlebar| titlebar.title),
            Some("ClankerDiff".into())
        );
    }
}
