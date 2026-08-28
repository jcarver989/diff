//! Native GPUI shell for reviewing changes in a local Git repository.

mod app;
mod args;

use app::DesktopApp;
use args::{ArgsError, CliArgs, USAGE};
use diff_gpui::DiffViewer;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use std::borrow::Cow;

const LILEX_REGULAR: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-Bold.ttf");
const LILEX_ITALIC: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-Italic.ttf");
const LILEX_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-BoldItalic.ttf");

fn main() {
    let args = match CliArgs::parse() {
        Ok(args) => args,
        Err(ArgsError::Help) => {
            println!("{USAGE}");
            return;
        }
        Err(error) => {
            eprintln!("error: {error}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    gpui_platform::application().run(move |cx: &mut App| {
        gpui_tokio::init(cx);
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(LILEX_REGULAR),
                Cow::Borrowed(LILEX_BOLD),
                Cow::Borrowed(LILEX_ITALIC),
                Cow::Borrowed(LILEX_BOLD_ITALIC),
            ])
            .expect("failed to load the bundled Lilex font");
        DiffViewer::bind_keys(cx);
        DesktopApp::bind_keys(cx);

        let bounds = Bounds::centered(None, size(px(1280.0), px(840.0)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Diff Review".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: true,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| DesktopApp::new(args, window, cx)),
        )
        .expect("failed to open the diff review window");
        cx.activate(true);
    });
}
