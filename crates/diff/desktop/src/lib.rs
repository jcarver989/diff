mod app;
pub mod args;
mod markdown_app;
mod menus;
pub mod preferences;

use args::CliArgs;
use clankerdiff_client::DiffClient;
use clankerdiff_core::ReviewSubmission;
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewSubmission};
use clankerdiff_rouge_shell::{
    launch,
    rouge::{App, WindowOptions, core::View},
    runtime,
};
use preferences::Preferences;
use std::sync::mpsc;

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
    let runtime = runtime::build();
    let handle = runtime.handle().clone();
    let (preferences, flush) = Preferences::load(&handle);
    let options =
        WindowOptions::default().title("ClankerDiff").size(1280.0, 840.0).min_size(640.0, 480.0).maximized(true);
    let menus =
        if diff { menus::all() } else { clankerdiff_rouge_shell::menus::build("ClankerDiff", Vec::new(), Vec::new()) };
    if let Err(error) = launch(App::new().menus(menus).window(options, move || {
        runtime::install(handle);
        root(preferences)
    })) {
        eprintln!("ClankerDiff: {error}");
    }
    runtime.block_on(async {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), flush).await;
    });
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
}
