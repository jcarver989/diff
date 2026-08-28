//! Native GPUI shell for reviewing changes in a local Git repository.

mod app;
mod args;
mod window_chrome;

use app::DesktopApp;
use args::{ArgsError, CliArgs, USAGE};
use diff_gpui::{DiffViewer, load_default_fonts};
use gpui::{
    App, AppContext, Bounds, Pixels, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

fn window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Diff Review".into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(640.0), px(480.0))),
        focus: true,
        is_movable: true,
        is_resizable: true,
        is_minimizable: true,
        window_decorations: None,
        ..Default::default()
    }
}

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
        load_default_fonts(cx).expect("failed to load the bundled fonts");
        DiffViewer::bind_keys(cx);
        DesktopApp::bind_keys(cx);

        let bounds = Bounds::centered(None, size(px(1280.0), px(840.0)), cx);
        cx.open_window(window_options(bounds), |window, cx| {
            cx.new(|cx| DesktopApp::new(args, window, cx))
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

        assert_eq!(options.window_bounds, Some(WindowBounds::Windowed(bounds)));
        assert_eq!(options.window_min_size, Some(size(px(640.0), px(480.0))));
        assert!(options.is_movable);
        assert!(options.is_resizable);
        assert!(options.is_minimizable);
        assert_eq!(options.window_decorations, None);
        assert_eq!(
            options.titlebar.and_then(|titlebar| titlebar.title),
            Some("Diff Review".into())
        );
    }
}
