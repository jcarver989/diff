//! Native GPUI shell for reviewing changes in a local Git repository.

use diff_gpui_desktop::{
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

    run(args);
}
