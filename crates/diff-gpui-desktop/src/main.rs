//! Native GPUI shell for reviewing changes in a local Git repository.

use clankerdiff_client::{DiffClient, DiffReviewEvent};
use clankerdiff_gpui_desktop::{
    args::{ArgsError, CliArgs, USAGE},
    run, run_client_review,
};

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

    let runtime = tokio::runtime::Runtime::new().expect("failed to start the async runtime");
    let client = args.connect.as_deref().map(|url| {
        runtime
            .block_on(DiffClient::connect(url, args.scope.into()))
            .unwrap_or_else(|error| {
                eprintln!("error: {error}");
                std::process::exit(1);
            })
    });
    if let Some(client) = client {
        let submission = run_client_review(client.clone());
        let event = submission.map_or(DiffReviewEvent::Cancel, DiffReviewEvent::SubmitReview);
        if let Err(error) = runtime.block_on(client.handle(event)) {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
        if let Err(error) = runtime.block_on(client.close()) {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    } else {
        run(args, None);
    }
}
