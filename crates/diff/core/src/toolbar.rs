use crate::{
    CommandEntry, DiffReviewCommand, DiffScope, FilesMode, LineStat, PageLabel, ReviewSession, TargetLocation,
    ViewMode, command_entry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Sidebar { hidden: bool },
    AllFiles { collapsed: bool },
    Back,
    Forward,
    PreviousCommit,
    NextCommit,
    ChooseCommit,
    Scope(DiffScope),
    Layout(ViewMode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarControl {
    pub kind: ControlKind,
    pub command: DiffReviewCommand,
    pub tooltip: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Toolbar {
    pub files: Vec<ToolbarControl>,
    pub history: Vec<ToolbarControl>,
    pub stepper: Vec<ToolbarControl>,
    pub page: Option<PageLabel>,
    pub scopes: Vec<ToolbarControl>,
    pub layouts: Vec<ToolbarControl>,
    pub stat: Option<LineStat>,
}

impl Toolbar {
    #[must_use]
    pub fn new(
        session: &ReviewSession,
        sidebar_hidden: Option<bool>,
        browsable: bool,
        key_label: impl Fn(&DiffReviewCommand) -> Option<String>,
    ) -> Self {
        let control = |kind, command: DiffReviewCommand, text: &str, selected| {
            let tooltip = tooltip(text, key_label(&command));
            ToolbarControl { kind, command, tooltip, selected }
        };
        let document = session.presentation().document();
        let has_files = !document.files.is_empty();
        let diff_view = has_files && session.source_view().is_none();
        let browsing = session.files_mode() == FilesMode::Browse;
        let mut toolbar = Self::default();
        if has_files || browsable {
            if let Some(hidden) = sidebar_hidden {
                let verb = if hidden { "Show" } else { "Hide" };
                let text = format!("{verb} {}", if browsing { "files" } else { "changed files" });
                toolbar.files.push(control(
                    ControlKind::Sidebar { hidden },
                    DiffReviewCommand::ToggleSidebar,
                    &text,
                    false,
                ));
            }
            if diff_view {
                let collapsed = session.all_files_collapsed();
                let text = if collapsed { "Expand all files" } else { "Collapse all files" };
                let kind = ControlKind::AllFiles { collapsed };
                toolbar.files.push(control(kind, DiffReviewCommand::ToggleAllFilesCollapsed, text, false));
            }
            let back = history_text("Back", session.back_location());
            let forward = history_text("Forward", session.forward_location());
            toolbar.history = vec![
                control(ControlKind::Back, DiffReviewCommand::NavigateBack, &back, false),
                control(ControlKind::Forward, DiffReviewCommand::NavigateForward, &forward, false),
            ];
            toolbar.stat = has_files.then(|| document.line_stat());
        }
        toolbar.page = session.page_label().filter(PageLabel::is_paged);
        if toolbar.page.is_some() {
            toolbar.stepper = vec![
                control(ControlKind::PreviousCommit, DiffReviewCommand::StepTarget(-1), "Previous commit", false),
                control(ControlKind::NextCommit, DiffReviewCommand::StepTarget(1), "Next commit", false),
                control(ControlKind::ChooseCommit, DiffReviewCommand::OpenCommitPicker, "Choose commit", false),
            ];
        }
        if let Some(selected) = session.target().scope() {
            toolbar.scopes = DiffScope::ALL
                .map(|scope| {
                    control(
                        ControlKind::Scope(scope),
                        DiffReviewCommand::SetScope(scope),
                        scope.title(),
                        scope == selected,
                    )
                })
                .to_vec();
        }
        if diff_view {
            let current = session.view_mode();
            let auto = if session.layout().is_split() {
                "Auto layout (showing split)"
            } else {
                "Auto layout (showing unified)"
            };
            toolbar.layouts =
                [(ViewMode::Auto, auto), (ViewMode::Unified, "Unified view"), (ViewMode::Split, "Split view")]
                    .map(|(mode, text)| {
                        control(ControlKind::Layout(mode), DiffReviewCommand::SetViewMode(mode), text, current == mode)
                    })
                    .to_vec();
        }
        toolbar
    }
}

#[must_use]
pub fn tooltip(text: &str, keys: Option<String>) -> String {
    keys.map_or_else(|| text.to_owned(), |keys| format!("{text} ({keys})"))
}

#[must_use]
pub fn default_key_label(command: &DiffReviewCommand) -> Option<String> {
    command_entry(command).and_then(CommandEntry::key_label)
}

fn history_text(direction: &str, target: Option<&TargetLocation>) -> String {
    target.map_or_else(
        || direction.to_owned(),
        |target| format!("{direction} to {}:{}", target.path.display_path().file_name(), target.line_number),
    )
}
