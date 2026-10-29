use crate::{CommitTitle, LineStat, ReviewTarget};
use clankerdiff_ui_core::{PickerOption, PickerState};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;
const MONTH: u64 = 30 * DAY;
const YEAR: u64 = 365 * DAY;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitPage {
    pub number: usize,
    pub target: ReviewTarget,
    pub short_id: Option<String>,
    pub subject: String,
    pub title: CommitTitle,
    pub body: String,
    pub stat: Option<LineStat>,
    pub age: Option<String>,
    pub comments: usize,
    pub current: bool,
}

impl CommitPage {
    #[must_use]
    pub const fn is_working_tree(&self) -> bool {
        matches!(self.target, ReviewTarget::WorkingTree(_))
    }

    #[must_use]
    pub fn id_label(&self) -> &str {
        self.short_id.as_deref().unwrap_or("working tree")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitPicker {
    pages: Vec<CommitPage>,
    picker: PickerState,
}

impl CommitPicker {
    #[must_use]
    pub fn new(pages: Vec<CommitPage>) -> Option<Self> {
        if pages.is_empty() {
            return None;
        }
        let options = pages.iter().map(|page| PickerOption::new(page.subject.clone())).collect();
        let mut picker = PickerState::new(options).unfiltered();
        if let Some(current) = pages.iter().position(|page| page.current) {
            picker.highlight(current);
        }
        Some(Self { pages, picker })
    }

    #[must_use]
    pub fn pages(&self) -> &[CommitPage] {
        &self.pages
    }

    #[must_use]
    pub const fn picker(&self) -> &PickerState {
        &self.picker
    }

    #[must_use]
    pub const fn highlighted(&self) -> usize {
        self.picker.highlighted_position()
    }

    pub fn move_by(&mut self, delta: isize) {
        self.picker.move_by(delta);
    }
}

#[must_use]
pub fn format_age(seconds: u64) -> String {
    let (count, unit) = match seconds {
        0..MINUTE => return "now".to_owned(),
        MINUTE..HOUR => (seconds / MINUTE, "m"),
        HOUR..DAY => (seconds / HOUR, "h"),
        DAY..WEEK => (seconds / DAY, "d"),
        WEEK..MONTH => (seconds / WEEK, "w"),
        MONTH..YEAR => (seconds / MONTH, "mo"),
        _ => (seconds / YEAR, "y"),
    };
    format!("{count}{unit}")
}
