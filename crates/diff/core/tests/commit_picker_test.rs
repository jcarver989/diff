use clankerdiff_core::{
    CommitPage, CommitPicker, CommitTitle, DiffScope, ReviewTarget, format_age, testing::commit_id,
};
use std::error::Error;

#[test]
fn the_picker_opens_on_the_current_page_and_stays_in_range() -> Result<(), Box<dyn Error>> {
    let mut picker = CommitPicker::new(pages(1)).ok_or("picker")?;
    assert_eq!(picker.highlighted(), 1);
    picker.move_by(-5);
    assert_eq!(picker.highlighted(), 0);
    picker.move_by(isize::MAX);
    assert_eq!(picker.highlighted(), 2);
    assert_eq!(CommitPicker::new(Vec::new()), None);
    Ok(())
}

#[test]
fn ages_are_compact_enough_for_a_column() {
    for (seconds, age) in [
        (5, "now"),
        (60, "1m"),
        (3 * 3600, "3h"),
        (86_400, "1d"),
        (4 * 86_400, "4d"),
        (21 * 86_400, "3w"),
        (60 * 86_400, "2mo"),
        (800 * 86_400, "2y"),
    ] {
        assert_eq!(format_age(seconds), age);
    }
}

fn pages(current: usize) -> Vec<CommitPage> {
    [ReviewTarget::Commit(commit_id(1)), ReviewTarget::Commit(commit_id(2)), DiffScope::Both.into()]
        .into_iter()
        .enumerate()
        .map(|(index, target)| CommitPage {
            number: index + 1,
            short_id: target.commit().map(|id| id.short().to_owned()),
            subject: format!("Page {}", index + 1),
            title: CommitTitle::parse(&format!("Page {}", index + 1)),
            body: String::new(),
            stat: None,
            age: None,
            comments: 0,
            current: index == current,
            target,
        })
        .collect()
}
