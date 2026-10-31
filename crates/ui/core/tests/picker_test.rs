use clankerdiff_ui_core::{PickerOption, PickerRow, PickerState};

#[test]
fn an_empty_query_keeps_every_option_in_order() {
    let mut state = PickerState::new(options(["b", "a", "c"]));
    state.set_query("  ");
    assert_eq!(state.matches(), [0, 1, 2]);
}

#[test]
fn matches_are_ranked_best_first_and_misses_dropped() {
    let mut state = PickerState::new(options(["Toggle split / unified", "Submit review", "Stage all"]));
    state.set_query("sub");
    assert_eq!(state.matches(), [1]);
    state.set_query("st");
    assert_eq!(state.matches()[0], 2);
    state.set_query("zzz");
    assert!(state.matches().is_empty() && state.highlighted().is_none());
}

#[test]
fn groups_stay_together_in_first_appearance_order_under_their_headers() {
    let state = PickerState::new(vec![
        PickerOption::new("next").group("Navigation"),
        PickerOption::new("theme").group("View"),
        PickerOption::new("previous").group("Navigation"),
    ]);
    assert_eq!(
        state.rows(),
        [
            PickerRow::Group("Navigation"),
            PickerRow::Option { position: 0, index: 0 },
            PickerRow::Option { position: 1, index: 2 },
            PickerRow::Group("View"),
            PickerRow::Option { position: 2, index: 1 },
        ]
    );
}

#[test]
fn the_highlight_skips_disabled_options_and_stops_at_either_end() {
    let mut state = PickerState::new(vec![
        PickerOption::new("off").disabled(true),
        PickerOption::new("first"),
        PickerOption::new("off").disabled(true),
        PickerOption::new("last"),
    ]);
    assert_eq!(state.highlighted(), Some(1));
    assert!(state.move_by(1));
    assert_eq!(state.highlighted(), Some(3));
    assert!(!state.move_by(8));
    assert!(state.move_by(-8));
    assert_eq!(state.highlighted(), Some(1));
    assert!(!state.highlight(2));
    assert!(state.highlight(3));
}

#[test]
fn an_unfiltered_picker_keeps_the_query_for_its_host_and_lists_everything() {
    let mut state = PickerState::new(options(["a", "b"])).unfiltered();
    state.set_query("zzz");
    assert_eq!(state.query(), "zzz");
    assert_eq!(state.matches(), [0, 1]);
}

#[test]
fn a_limited_picker_keeps_only_its_best_matches() {
    let texts = ["Submit review", "Stage all", "Show split", "Toggle split / unified"];
    let mut everything = PickerState::new(options(texts));
    let mut limited = PickerState::new(options(texts)).limit(2);
    assert_eq!(limited.matches(), [0, 1]);

    everything.set_query("s");
    limited.set_query("s");
    assert_eq!(limited.matches(), &everything.matches()[..2]);
}

fn options<const N: usize>(texts: [&str; N]) -> Vec<PickerOption> {
    texts.into_iter().map(PickerOption::new).collect()
}
