use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use diff_core::DiffScope;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Agent-driven code review for Git working tree changes"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Review changes and return the submitted feedback on stdout.
    Review(ReviewArgs),
    /// Review a rendered Markdown document.
    Markdown(MarkdownArgs),
    /// Attach the TUI to an active review session.
    #[command(hide = true)]
    Attach(AttachArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, ClapArgs)]
pub struct AttachArgs {
    pub session_url: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Ui {
    Tui,
    Desktop,
    #[default]
    Web,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, ClapArgs)]
pub struct MarkdownArgs {
    /// Markdown file to review, or `-` to read UTF-8 Markdown from stdin.
    pub path: String,

    /// Optional display title. Defaults to the first H1, then the file name.
    #[arg(long)]
    pub title: Option<String>,

    /// User interface used for the review.
    #[arg(long, value_enum, default_value_t = Ui::Tui)]
    pub ui: Ui,

    /// Feedback format written to stdout.
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, ClapArgs)]
pub struct ReviewArgs {
    /// Repository or a path contained by it.
    #[arg(default_value = ".")]
    pub repository: PathBuf,

    /// Diff scope to review.
    #[arg(short, long, default_value = "both")]
    pub scope: DiffScope,

    /// User interface used for the review.
    ///
    /// TUI reviews require `CLANKERDIFF_TUI_COMMAND`. Clankerdiff executes that command and appends
    /// `<current-clankerdiff-executable> attach <session-url>` to its arguments. For example:
    /// `CLANKERDIFF_TUI_COMMAND='ghostty +new-window -e'`.
    #[arg(long, value_enum, default_value_t)]
    pub ui: Ui,

    /// Feedback format written to stdout.
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,

    /// Web server port; zero chooses an ephemeral port.
    #[arg(long, default_value_t = 0)]
    pub port: u16,

    /// Print the web review URL to stderr without opening a browser.
    #[arg(long)]
    pub no_open: bool,

    /// Use an external Trunk build instead of the embedded web application.
    #[arg(long, value_name = "PATH")]
    pub web_assets: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn review_args(arguments: impl IntoIterator<Item = &'static str>) -> ReviewArgs {
        let cli = Cli::try_parse_from(arguments).expect("arguments should parse");
        match cli.command {
            Command::Review(args) => args,
            Command::Attach(_) | Command::Markdown(_) => panic!("expected the review command"),
        }
    }

    #[test]
    fn parses_review_defaults() {
        let args = review_args(["clankerdiff", "review"]);
        assert_eq!(args.repository, PathBuf::from("."));
        assert_eq!(args.scope, DiffScope::Both);
        assert_eq!(args.ui, Ui::Web);
        assert_eq!(args.format, OutputFormat::Text);
    }

    #[test]
    fn parses_all_review_options() {
        let args = review_args([
            "clankerdiff",
            "review",
            "--ui=tui",
            "--scope",
            "staged",
            "--format=json",
            "--port",
            "8123",
            "--no-open",
            "--web-assets",
            "/tmp/web",
            "/tmp/repo",
        ]);
        assert_eq!(args.ui, Ui::Tui);
        assert_eq!(args.scope, DiffScope::Staged);
        assert_eq!(args.format, OutputFormat::Json);
        assert_eq!(args.port, 8123);
        assert!(args.no_open);
        assert_eq!(args.repository, PathBuf::from("/tmp/repo"));
        assert_eq!(args.web_assets, Some(PathBuf::from("/tmp/web")));
    }

    #[test]
    fn parses_markdown_path_and_stdin() {
        let cli = Cli::try_parse_from([
            "clankerdiff",
            "markdown",
            "plan.md",
            "--title",
            "Plan",
            "--ui=desktop",
            "--format=json",
        ])
        .unwrap();
        let Command::Markdown(args) = cli.command else {
            panic!("markdown command")
        };
        assert_eq!(args.path, "plan.md");
        assert_eq!(args.title.as_deref(), Some("Plan"));
        assert_eq!(args.ui, Ui::Desktop);
        assert_eq!(args.format, OutputFormat::Json);

        let cli = Cli::try_parse_from(["clankerdiff", "markdown", "-"]).unwrap();
        let Command::Markdown(args) = cli.command else {
            panic!("markdown command")
        };
        assert_eq!(args.path, "-");
        assert_eq!(args.ui, Ui::Tui);
    }

    #[test]
    fn rejects_unknown_command_and_values() {
        let command = Cli::try_parse_from(["clankerdiff", "show"]).unwrap_err();
        assert_eq!(command.kind(), ErrorKind::InvalidSubcommand);

        let ui = Cli::try_parse_from(["clankerdiff", "review", "--ui", "other"]).unwrap_err();
        assert_eq!(ui.kind(), ErrorKind::InvalidValue);

        let port = Cli::try_parse_from(["clankerdiff", "review", "--port", "nope"]).unwrap_err();
        assert_eq!(port.kind(), ErrorKind::ValueValidation);
    }
}
