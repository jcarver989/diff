use clankerdiff_core::{
    Chord, CommandEntry, DiffReviewCommand, DiffSide, FocusPane, Key, KeyScope, KeyState, RevealAmount, ReviewCommand,
    diff_commands, diff_shortcuts,
};

#[test]
fn the_pane_and_layout_choose_what_a_key_does() {
    let files = KeyState { split: true, ..KeyState::default() };
    let unified = KeyState { document: true, ..KeyState::default() };
    let split = KeyState { document: true, split: true, ..KeyState::default() };
    assert_eq!(command(Chord::new(Key::Left), files), Some(DiffReviewCommand::CollapseSelected));
    assert_eq!(command(Chord::new(Key::Left), unified), Some(DiffReviewCommand::Focus(FocusPane::Files)));
    assert_eq!(command(Chord::new(Key::Left), split), Some(DiffReviewCommand::SelectSide(DiffSide::Old)));
    assert_eq!(command(Chord::new(Key::Right), unified), None);
    assert_eq!(command(Chord::new(Key::Enter), files), Some(DiffReviewCommand::OpenSelected));
    assert_eq!(command(Chord::new(Key::Enter), split), Some(DiffReviewCommand::RevealGap(RevealAmount::Step)));
    assert_eq!(command(Chord::char(' '), split), None);
}

#[test]
fn an_open_card_and_a_reference_take_over_their_keys() {
    let diff = KeyState { document: true, ..KeyState::default() };
    let card = KeyState { card: true, ..diff };
    let reference = KeyState { reference: true, ..diff };
    assert_eq!(command(Chord::new(Key::Esc), diff), Some(ReviewCommand::Cancel.into()));
    assert_eq!(command(Chord::new(Key::Esc), reference), Some(DiffReviewCommand::NavigateBack));
    assert_eq!(
        command(Chord::new(Key::Esc), KeyState { card: true, ..reference }),
        Some(DiffReviewCommand::CloseDefinition)
    );
    assert_eq!(command(Chord::new(Key::Enter), card), Some(DiffReviewCommand::GoToDefinition));
    assert_eq!(command(Chord::new(Key::Tab), card), Some(DiffReviewCommand::NextDefinition));
    assert_eq!(command(Chord::new(Key::Tab), diff), Some(DiffReviewCommand::ToggleFocus));
}

#[test]
fn no_two_commands_claim_the_same_key_in_the_same_state() {
    let states = (0..16_u8).map(|bits| KeyState {
        document: bits & 1 != 0,
        split: bits & 2 != 0,
        card: bits & 4 != 0,
        reference: bits & 8 != 0,
    });
    for state in states {
        let mut claimed: Vec<(Chord, u8, &str)> = Vec::new();
        for (chord, scope, entry) in diff_shortcuts() {
            if !scope.matches(state) {
                continue;
            }
            let rank = scope.precedence();
            if let Some((_, _, other)) = claimed
                .iter()
                .find(|(key, claimed_rank, title)| *key == chord && *claimed_rank == rank && *title != entry.title)
            {
                panic!("{} and {other} both claim {} in {state:?}", entry.title, chord.label());
            }
            claimed.push((chord, rank, entry.title));
        }
    }
}

#[test]
fn shortcuts_are_ordered_from_general_to_specific() {
    let ranks: Vec<u8> = diff_shortcuts().iter().map(|(_, scope, _)| scope.precedence()).collect();
    assert!(ranks.is_sorted());
}

#[test]
fn every_command_has_a_unique_title() {
    let mut titles: Vec<&str> = diff_commands().iter().map(|entry| entry.title).collect();
    let count = titles.len();
    titles.sort_unstable();
    titles.dedup();
    assert_eq!(titles.len(), count);
}

#[test]
fn key_labels_read_like_the_keys_on_a_keyboard() {
    assert_eq!(Chord::ctrl('o').label(), "Ctrl-O");
    assert_eq!(Chord::char(' ').label(), "Space");
    assert_eq!(Chord::new(Key::PageDown).label(), "PgDn");
    let next = diff_commands().iter().find(|entry| entry.title == "Next item");
    assert_eq!(next.and_then(CommandEntry::key_label).as_deref(), Some("j / ↓"));
    let forward = diff_commands().iter().find(|entry| entry.title == "Forward");
    assert_eq!(forward.and_then(CommandEntry::key_label), None);
}

fn command(chord: Chord, state: KeyState) -> Option<DiffReviewCommand> {
    let candidates = diff_shortcuts().into_iter().filter(|(key, _, _)| *key == chord);
    KeyScope::resolve(state, candidates.map(|(_, scope, entry)| (scope, entry))).map(|entry| entry.command.clone())
}
