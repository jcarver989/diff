//! Native GPUI shell for reviewing changes in a local Git repository.

use clankerdiff_client::DiffClient;
use clankerdiff_gpui_desktop::{
    args::{ArgsError, CliArgs, USAGE},
    run,
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
    run(args, client);
}
