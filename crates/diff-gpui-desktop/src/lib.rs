//! Native GPUI host for standalone and agent-driven diff review sessions.

mod app;
pub mod args;
mod markdown_app;
mod menus;
pub mod preferences;

use app::DesktopApp;
use args::CliArgs;
use clankerdiff_client::DiffClient;
use clankerdiff_core::ReviewSubmission;
use clankerdiff_gpui::{
    DiffViewer, MarkdownReviewer,
    ui::{InterfaceSize, fonts::load_fonts, interface_size::DEFAULT_INTERFACE_SIZE},
};
use clankerdiff_gpui_shell::{run_loop, runtime};
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewSubmission};
use gpui::{
    App, AppContext, Bounds, Pixels, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use markdown_app::MarkdownDesktopApp;
use std::sync::mpsc;

/// The title the client-side title bar shows.
const WINDOW_TITLE: &str = "Diff Review";

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
pub fn run(args: CliArgs, client: Option<DiffClient>) {
    run_application(args, client, None);
}

/// Runs a one-shot desktop review and returns its submitted feedback.
///
/// Closing or cancelling the window returns `None`.
#[must_use]
pub fn run_review(args: CliArgs) -> Option<ReviewSubmission> {
    let (sender, receiver) = mpsc::channel();
    run_application(args, None, Some(sender));
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
        load_fonts(cx).expect("failed to load the bundled fonts");
        InterfaceSize::init(DEFAULT_INTERFACE_SIZE, cx);
        MarkdownReviewer::bind_keys(cx);
        menus::install(cx);
        run_loop::install(cx);
        let bounds = Bounds::centered(None, size(px(1280.0), px(840.0)), cx);
        cx.open_window(window_options(bounds), |window, cx| {
            cx.new(|cx| MarkdownDesktopApp::new(std::sync::Arc::new(document), sender, window, cx))
        })
        .expect("failed to open the Markdown review window");
        cx.activate(true);
    });
    receiver.try_recv().ok().flatten()
}

#[must_use]
pub fn run_client_review(client: DiffClient) -> Option<ReviewSubmission> {
    let (sender, receiver) = mpsc::channel();
    run_application(CliArgs::default(), Some(client), Some(sender));
    receiver.try_recv().ok().flatten()
}

fn run_application(
    args: CliArgs,
    client: Option<DiffClient>,
    outcome_sender: Option<mpsc::Sender<Option<ReviewSubmission>>>,
) {
    let runtime = runtime::build();
    let handle = runtime.handle().clone();
    gpui_platform::application().run(move |cx: &mut App| {
        runtime::install(cx, handle);
        load_fonts(cx).expect("failed to load the bundled fonts");
        InterfaceSize::init(DEFAULT_INTERFACE_SIZE, cx);
        DiffViewer::bind_keys(cx);
        DesktopApp::bind_keys(cx);
        menus::install(cx);
        run_loop::install(cx);

        let bounds = Bounds::centered(None, size(px(1280.0), px(840.0)), cx);
        cx.open_window(window_options(bounds), |window, cx| {
            cx.new(|cx| DesktopApp::new(args, client, outcome_sender, window, cx))
        })
        .expect("failed to open the diff review window");
        cx.activate(true);
    });
    runtime.shutdown_background();
}

pub use clankerdiff_gpui_shell::menus::{About, Hide, HideOthers, Minimize, Quit, ShowAll, Zoom};
pub use menus::SetScope;

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{OwnedMenuItem, point, size};

    fn action_names(menu: &gpui::OwnedMenu) -> Vec<String> {
        menu.items
            .iter()
            .filter_map(|item| match item {
                OwnedMenuItem::Action { action, .. } => Some(action.name().to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn desktop_menus_expose_app_edit_and_window_menus() {
        let menus: Vec<gpui::OwnedMenu> =
            clankerdiff_gpui_shell::menus::build(menus::APP_NAME, Vec::new(), menus::extra())
                .into_iter()
                .map(gpui::Menu::owned)
                .collect();
        let names: Vec<&str> = menus.iter().map(|menu| menu.name.as_ref()).collect();
        assert_eq!(names, vec!["ClankerDiff", "Edit", "View", "Window"]);

        let app_actions = action_names(&menus[0]);
        assert!(app_actions.contains(&"app_menu::About".to_owned()));
        assert!(app_actions.contains(&"app_menu::Quit".to_owned()));
        assert!(app_actions.contains(&"app_menu::Hide".to_owned()));
        assert!(app_actions.contains(&"app_menu::HideOthers".to_owned()));
        assert!(app_actions.contains(&"app_menu::ShowAll".to_owned()));
        assert!(
            menus[0]
                .items
                .iter()
                .any(|item| matches!(item, OwnedMenuItem::SystemMenu(_)))
        );

        let edit_os_actions: Vec<Option<gpui::OsAction>> = menus[1]
            .items
            .iter()
            .filter_map(|item| match item {
                OwnedMenuItem::Action { os_action, .. } => Some(*os_action),
                _ => None,
            })
            .collect();
        for expected in [
            gpui::OsAction::Undo,
            gpui::OsAction::Redo,
            gpui::OsAction::Cut,
            gpui::OsAction::Copy,
            gpui::OsAction::Paste,
            gpui::OsAction::SelectAll,
        ] {
            assert!(edit_os_actions.contains(&Some(expected)));
        }

        let view_actions = action_names(&menus[2]);
        assert!(view_actions.contains(&"desktop_menu::SetScope".to_owned()));

        let window_actions = action_names(&menus[3]);
        assert!(window_actions.contains(&"app_menu::Minimize".to_owned()));
        assert!(window_actions.contains(&"app_menu::Zoom".to_owned()));
    }

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
