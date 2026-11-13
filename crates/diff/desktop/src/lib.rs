mod app;
pub mod args;
mod markdown_app;
mod menus;
pub mod preferences;

use args::CliArgs;
use clankerdiff_client::DiffClient;
use clankerdiff_core::ReviewSubmission;
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewSubmission};
use preferences::Preferences;
use rouge::{App, Menu, WindowOptions, core::View};
use rouge_widgets::bundled_fonts;
use std::sync::mpsc;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::time::timeout;

pub fn run(args: CliArgs, client: Option<DiffClient>) {
    run_application(args, client, None);
}

#[must_use]
pub fn run_review(args: CliArgs) -> Option<ReviewSubmission> {
    let (sender, receiver) = mpsc::channel();
    run_application(args, None, Some(sender));
    receiver.try_recv().ok().flatten()
}

#[must_use]
pub fn run_client_review(client: DiffClient) -> Option<ReviewSubmission> {
    let (sender, receiver) = mpsc::channel();
    run_application(CliArgs::default(), Some(client), Some(sender));
    receiver.try_recv().ok().flatten()
}

#[must_use]
pub fn run_markdown_review(document: MarkdownDocument) -> Option<MarkdownReviewSubmission> {
    let (sender, receiver) = mpsc::channel();
    run_window(move |preferences| markdown_app::root(document, sender, preferences), false);
    receiver.try_recv().ok().flatten()
}

fn run_application(args: CliArgs, client: Option<DiffClient>, outcome: Option<mpsc::Sender<Option<ReviewSubmission>>>) {
    run_window(move |preferences| app::root(args, client, outcome, preferences), true);
}

fn run_window<T: View>(root: impl FnOnce(Preferences) -> T + 'static, diff: bool) {
    let runtime = match Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => return eprintln!("ClankerDiff: failed to start the Tokio runtime: {error}"),
    };
    let handle = runtime.handle().clone();
    let (preferences, flush) = Preferences::load(&handle);
    let options =
        WindowOptions::default().title("ClankerDiff").size(1280.0, 840.0).min_size(640.0, 480.0).maximized(true);
    let menus = if diff { menus::all() } else { vec![Menu::app("ClankerDiff", Vec::new())] };
    let app = App::new().fonts(bundled_fonts()).tokio(handle).menus(menus).window(options, move || root(preferences));
    if let Err(error) = app.run() {
        eprintln!("ClankerDiff: {error}");
    }
    runtime.block_on(async {
        let _ = timeout(Duration::from_secs(2), flush).await;
    });
    runtime.shutdown_timeout(Duration::from_secs(2));
}
