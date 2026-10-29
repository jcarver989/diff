use clankerdiff_core::{
    DefinitionRequest, DiffDocument, DiffSide, Fingerprint, Lookup, LookupAnswer, LookupFailure, LookupReply,
    LookupRequest, LookupStatus, LookupUnavailable, RepoPath, Resolution, ReviewSession, SourceLocation,
    TargetLocation,
    testing::{DocumentBuilder, ResolutionBuilder, external, location, source, target},
};
use clankerdiff_symbols::{Activation, DefinitionCard, Hold, NameLookup, Settled, SymbolCache, SymbolIndex};
use std::{error::Error, sync::Arc};

#[test]
fn a_line_with_one_name_opens_its_card_directly() -> Result<(), Box<dyn Error>> {
    let (session, index) = fixture();
    let mut names = NameLookup::default();
    assert!(names.open_at(location("src/main.rs", 3)?, &session, &index, false));
    assert_eq!(card(&names)?.name, "helper");
    assert_eq!(card(&names)?.target(&index), Some(target("src/a/util.rs", 1)?));
    Ok(())
}

#[test]
fn opening_again_on_the_same_line_steps_through_its_distinct_names() -> Result<(), Box<dyn Error>> {
    let (session, index) = fixture();
    let mut names = NameLookup::default();
    let line = location("src/main.rs", 2)?;
    let mut opened = Vec::new();
    for _ in 0..3 {
        assert!(names.open_at(line.clone(), &session, &index, false));
        opened.push(card(&names)?.name.clone());
    }
    assert_eq!(opened, ["Cache", "build", "Cache"]);
    assert_eq!(card(&names)?.origin, line);
    Ok(())
}

#[test]
fn a_line_without_names_has_nothing_to_look_up() -> Result<(), Box<dyn Error>> {
    let (session, index) = fixture();
    let mut names = NameLookup::default();
    assert!(!names.open_at(location("src/main.rs", 5)?, &session, &index, false));
    assert!(names.card().is_none());
    Ok(())
}

#[test]
fn cycling_walks_definitions_nearest_first_and_wraps() -> Result<(), Box<dyn Error>> {
    let (session, index) = fixture();
    let mut names = NameLookup::default();
    assert!(names.open_at(location("src/main.rs", 3)?, &session, &index, false));
    let mut visited = Vec::new();
    for _ in 0..3 {
        visited.push(card(&names)?.content(&index).place.ok_or("a definition")?);
        assert!(names.cycle(&index));
    }
    assert_eq!(visited, ["src/a/util.rs:1 · 1 of 2", "lib/util.rs:1 · 2 of 2", "src/a/util.rs:1 · 1 of 2"]);
    Ok(())
}

#[test]
fn a_name_defined_once_or_not_at_all_does_not_cycle() -> Result<(), Box<dyn Error>> {
    let (session, index) = fixture();
    let mut names = NameLookup::default();
    assert!(names.open_at(location("src/main.rs", 2)?, &session, &index, false));
    assert!(!names.cycle(&index));
    assert_eq!(card(&names)?.content(&index).message.as_deref(), Some("No definition of Cache in this diff"));
    assert_eq!(names.activate(&session, &index, true), Activation::Idle);
    assert!(names.open_at(location("src/main.rs", 2)?, &session, &index, false));
    assert_eq!(card(&names)?.name, "build");
    assert!(!names.cycle(&index));
    Ok(())
}

#[test]
fn only_the_card_name_is_marked_on_its_line() -> Result<(), Box<dyn Error>> {
    let (session, index) = fixture();
    let mut names = NameLookup::default();
    let line = location("src/main.rs", 2)?;
    assert!(names.open_at(line.clone(), &session, &index, false));
    assert_eq!(names.card_columns(&line), Some(16..21));
    assert_eq!(names.card_columns(&location("src/main.rs", 3)?), None);
    assert!(names.close());
    assert_eq!(names.card_columns(&line), None);
    Ok(())
}

#[test]
fn a_hovered_card_lingers_once_neither_its_name_nor_the_card_is_hovered() -> Result<(), Box<dyn Error>> {
    let (session, index) = fixture();
    let mut names = NameLookup::default();
    let line = location("src/main.rs", 3)?;
    let reference = index.references_at(&line).first().ok_or("helper is referenced")?.clone();
    names.show(DefinitionCard::new(line.clone(), &reference), &session, false, Hold::Hover);
    assert!(!names.lingering());
    names.hover_token(false);
    names.hover_card(true);
    assert!(!names.lingering());
    names.hover_card(false);
    assert!(names.lingering());
    assert!(names.cycle(&index));
    assert!(!names.lingering(), "cycling pins the card");

    assert!(names.open_at(line, &session, &index, false));
    names.hover_token(false);
    assert!(!names.lingering(), "a card opened from the keyboard stays pinned");
    Ok(())
}

#[test]
fn a_language_server_answer_replaces_the_candidates_matched_by_name() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let mut names = helper(&session, &index)?;
    let request = sent(&mut names)?;
    let content = main_content(&session)?;
    let expected = DefinitionRequest { at: location("src/main.rs", 3)?, column: 4, content };
    assert_eq!(request.request, LookupRequest::Definition(expected));
    let pending = card(&names)?.content(&index);
    assert_eq!(pending.place.as_deref(), Some("src/a/util.rs:1 · 1 of 2"));
    assert_eq!(pending.uses, "looking up · 2 uses in this diff");

    let answer = ResolutionBuilder::new().external(3, STD, 4).hover("fn helper()").answer();
    assert_eq!(names.finish(reply(&request, Ok(answer)), &mut session, &index), Settled::Redraw);
    let content = card(&names)?.content(&index);
    assert_eq!((content.docs, content.place.as_deref()), (Some("fn helper()"), Some("std · alloc/src/util.rs:4")));
    assert_eq!((content.can_jump, content.can_cycle), (true, false));
    assert!(!names.cycle(&index));
    assert_eq!(names.activate(&session, &index, true), Activation::Jump(std_target()));
    Ok(())
}

#[test]
fn an_empty_answer_matches_by_name_until_reopening_the_card_asks_again() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let mut names = helper(&session, &index)?;
    let request = sent(&mut names)?;
    let unavailable = Ok(LookupAnswer::Definition(Resolution::default()));
    names.finish(reply(&request, unavailable), &mut session, &index);
    let content = card(&names)?.content(&index);
    assert_eq!(content.place.as_deref(), Some("src/a/util.rs:1 · 1 of 2"));
    assert_eq!(content.uses, "matched by name · 2 uses in this diff");
    assert!(content.can_cycle);

    assert!(names.open_at(location("src/main.rs", 3)?, &session, &index, true));
    let retry = sent(&mut names)?;
    assert_eq!(retry.request, request.request);
    let answer = Ok(ResolutionBuilder::new().external(3, STD, 4).answer());
    names.finish(reply(&retry, answer), &mut session, &index);
    assert_eq!(names.activate(&session, &index, true), Activation::Jump(std_target()));

    assert!(names.open_at(location("src/main.rs", 3)?, &session, &index, true));
    assert_eq!(names.take_lookups(), NONE);
    Ok(())
}

#[test]
fn docs_for_the_resolved_symbol_are_not_attached_to_definitions_matched_by_name() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let mut names = helper(&session, &index)?;
    let request = sent(&mut names)?;
    let answer = Ok(ResolutionBuilder::new().hover("Docs for some other helper").answer());
    names.finish(reply(&request, answer), &mut session, &index);
    let content = card(&names)?.content(&index);
    assert_eq!((content.place.as_deref(), content.docs), (Some("src/a/util.rs:1 · 1 of 2"), None));
    assert!(content.uses.starts_with("matched by name"));
    Ok(())
}

#[test]
fn a_late_answer_keeps_the_candidate_the_reviewer_cycled_to() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let mut names = helper(&session, &index)?;
    let request = sent(&mut names)?;
    assert!(names.cycle(&index));
    let answer = Ok(ResolutionBuilder::new().external(3, STD, 4).hover("Docs for the std helper").answer());
    names.finish(reply(&request, answer), &mut session, &index);
    let content = card(&names)?.content(&index);
    assert!(content.uses.starts_with("matched by name"));
    assert_eq!((content.place.as_deref(), content.docs), (Some("lib/util.rs:1 · 2 of 2"), None));
    assert_eq!(names.activate(&session, &index, true), Activation::Jump(target("lib/util.rs", 1)?));

    assert!(names.open_at(location("src/main.rs", 3)?, &session, &index, true));
    assert_eq!(names.take_lookups(), NONE);
    assert_eq!(names.activate(&session, &index, true), Activation::Jump(std_target()));
    Ok(())
}

#[test]
fn a_followed_card_jumps_once_its_answer_settles() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let mut names = helper(&session, &index)?;
    let request = sent(&mut names)?;
    assert_eq!(names.activate(&session, &index, true), Activation::Wait);
    let answer = Ok(ResolutionBuilder::new().target("src/a/util.rs", 1).preview("fn helper() {}").answer());
    let settled = names.finish(reply(&request, answer), &mut session, &index);
    assert_eq!(settled, Settled::Jump(target("src/a/util.rs", 1)?));

    assert!(names.open_at(location("src/main.rs", 2)?, &session, &index, true));
    names.take_lookups();
    assert!(names.open_at(location("src/main.rs", 2)?, &session, &index, true));
    let request = sent(&mut names)?;
    assert_eq!(names.activate(&session, &index, true), Activation::Wait);
    let failed = names.finish(reply(&request, Err("timed out".into())), &mut session, &index);
    assert_eq!(failed, Settled::Jump(target("src/a/util.rs", 2)?));
    assert_eq!(names.activate(&session, &index, true), Activation::Jump(target("src/a/util.rs", 2)?));
    assert_eq!(names.take_lookups(), NONE, "a failed card with a match by name jumps rather than asking again");
    Ok(())
}

#[test]
fn added_lines_on_the_old_side_disabled_lookups_and_cleared_answers_are_not_looked_up() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let mut names = NameLookup::default();
    let reference = index.references_at(&location("src/main.rs", 3)?).first().ok_or("helper")?.clone();
    let old = SourceLocation { side: DiffSide::Old, ..location("src/main.rs", 3)? };
    names.show(DefinitionCard::new(old, &reference), &session, true, Hold::Pinned);
    assert_eq!(names.take_lookups(), NONE);
    assert!(!card(&names)?.is_pending());
    assert_eq!(names.activate(&session, &index, true), Activation::Jump(target("src/a/util.rs", 1)?));
    assert!(names.open_at(location("src/main.rs", 3)?, &session, &index, false));
    assert_eq!(names.take_lookups(), NONE);

    let mut names = helper(&session, &index)?;
    let request = sent(&mut names)?;
    assert!(names.open_at(location("src/main.rs", 3)?, &session, &index, true));
    assert_eq!(names.take_lookups(), NONE);
    names.document_changed(&mut session, true);
    assert_eq!(names.take_lookups(), NONE);
    assert!(names.card().is_none());
    let answer = Ok(ResolutionBuilder::new().external(3, STD, 4).answer());
    assert_eq!(names.finish(reply(&request, answer), &mut session, &index), Settled::Ignored);
    Ok(())
}

#[test]
fn a_reference_source_opens_read_only_refreshes_and_says_why_it_cannot() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let target = TargetLocation { line_number: 2, ..std_target() };
    assert!(session.jump_to(target.clone()));
    let mut names = NameLookup::default();
    names.navigated(&mut session, false);
    assert_eq!(names.take_lookups(), NONE);
    assert_eq!(
        session.reference_status().as_deref(),
        Some("Couldn't open std · alloc/src/util.rs: definitions outside the diff need a connected repository")
    );

    names.navigated(&mut session, true);
    let request = sent(&mut names)?;
    assert_eq!(request.request, LookupRequest::Source(target.path.clone()));
    let other = Lookup { token: 99, request: LookupRequest::Source(external(9, "elsewhere.rs")) };
    assert_eq!(names.finish(reply(&other, Err("gone".into())), &mut session, &index), Settled::Ignored);
    names.finish(reply(&request, Err("not readable".into())), &mut session, &index);
    assert_eq!(session.reference_status().as_deref(), Some("Couldn't open std · alloc/src/util.rs: not readable"));

    names.navigated(&mut session, true);
    let retry = sent(&mut names)?;
    let opened = names.finish(reply(&retry, Ok(LookupAnswer::Source(source("a\nb\n")))), &mut session, &index);
    assert_eq!(opened, Settled::Opened);
    assert!(session.reference_status().is_none());
    assert_eq!(session.current_location(), Some(target));
    names.navigated(&mut session, true);
    assert_eq!(names.take_lookups(), NONE);
    names.document_changed(&mut session, true);
    assert_eq!(sent(&mut names)?.request, request.request);
    Ok(())
}

#[test]
fn answers_that_arrive_out_of_order_or_after_a_new_document_are_ignored() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let mut names = NameLookup::default();
    assert!(session.jump_to(std_target()));
    names.navigated(&mut session, true);
    let older = sent(&mut names)?;
    names.navigated(&mut session, true);
    let newer = sent(&mut names)?;
    let opened = names.finish(reply(&newer, Ok(LookupAnswer::Source(source("new\n")))), &mut session, &index);
    assert_eq!(opened, Settled::Opened);
    let stale = names.finish(reply(&older, Ok(LookupAnswer::Source(source("old\n")))), &mut session, &index);
    assert_eq!(stale, Settled::Ignored);
    let shown = session.reference().and_then(|file| file.source_document(DiffSide::New)).ok_or("the reference")?;
    assert_eq!(shown.text(), "new\n");

    let mut names = helper(&session, &index)?;
    let before = sent(&mut names)?;
    names.document_changed(&mut session, true);
    names.take_lookups();
    assert!(names.open_at(location("src/main.rs", 3)?, &session, &index, true));
    let after = sent(&mut names)?;
    assert_eq!(before.request, after.request);
    let answer = Ok(ResolutionBuilder::new().external(3, STD, 4).answer());
    assert_eq!(names.finish(reply(&before, answer.clone()), &mut session, &index), Settled::Ignored);
    assert!(card(&names)?.is_pending());
    assert_eq!(names.finish(reply(&after, answer), &mut session, &index), Settled::Redraw);
    assert_eq!(names.activate(&session, &index, true), Activation::Jump(std_target()));
    Ok(())
}

#[test]
fn a_hover_only_answer_shows_its_docs_instead_of_claiming_there_is_no_definition() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let (mut names, request) = cache_card(&session, &index)?;
    let answer = Ok(ResolutionBuilder::new().hover("pub struct Cache").answer());
    assert_eq!(names.finish(reply(&request, answer), &mut session, &index), Settled::Redraw);
    let content = card(&names)?.content(&index);
    assert_eq!((content.message, content.docs), (None, Some("pub struct Cache")));
    assert_eq!(content.uses, "docs only · 1 use in this diff");
    assert!(!content.can_jump);
    Ok(())
}

#[test]
fn a_failed_lookup_keeps_its_docs_says_why_and_retries_on_go_to_definition() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let (mut names, request) = cache_card(&session, &index)?;
    let failed =
        ResolutionBuilder::new().hover("pub struct Cache").status(LookupStatus::Failed(LookupFailure::TimedOut));
    names.finish(reply(&request, Ok(failed.answer())), &mut session, &index);
    let content = card(&names)?.content(&index);
    let expected = "Couldn't look up Cache: the language server didn't answer in time. Go to definition to retry";
    assert_eq!((content.message.as_deref(), content.docs), (Some(expected), Some("pub struct Cache")));
    assert!(content.can_jump);

    assert_eq!(names.activate(&session, &index, true), Activation::Wait);
    let retry = sent(&mut names)?;
    assert_eq!(retry.request, request.request);
    assert!(card(&names)?.is_pending());
    let answer = Ok(ResolutionBuilder::new().external(3, STD, 4).answer());
    assert_eq!(names.finish(reply(&retry, answer), &mut session, &index), Settled::Jump(std_target()));
    Ok(())
}

#[test]
fn failures_and_empty_answers_are_asked_again_while_definitive_answers_are_reused() -> Result<(), Box<dyn Error>> {
    let (mut session, index) = fixture();
    let (mut names, request) = cache_card(&session, &index)?;
    names.finish(reply(&request, Err("the daemon went away".into())), &mut session, &index);
    let message = card(&names)?.content(&index).message;
    assert_eq!(message.as_deref(), Some("Couldn't look up Cache: the daemon went away. Go to definition to retry"));
    let request = reopen(&mut names, &session, &index, &request)?;
    names.finish(reply(&request, Ok(LookupAnswer::Definition(Resolution::default()))), &mut session, &index);
    assert_eq!(card(&names)?.content(&index).message.as_deref(), Some("No definition of Cache found"));
    let request = reopen(&mut names, &session, &index, &request)?;
    let unavailable = Resolution::unavailable(LookupUnavailable::ChangedSinceReview);
    names.finish(reply(&request, Ok(LookupAnswer::Definition(unavailable))), &mut session, &index);
    let expected = "No definition of Cache in this diff; the file on disk has changed since this version";
    assert_eq!(card(&names)?.content(&index).message.as_deref(), Some(expected));
    names.close();
    assert!(names.open_at(location("src/main.rs", 2)?, &session, &index, true));
    assert_eq!(names.take_lookups(), NONE);
    assert_eq!(card(&names)?.content(&index).message.as_deref(), Some(expected));
    Ok(())
}

#[test]
fn an_unchanged_old_side_line_is_looked_up_in_the_current_version() -> Result<(), Box<dyn Error>> {
    let mut session = ReviewSession::new(revisions());
    let index = SymbolCache::default().index(&revisions());
    let mut names = NameLookup::default();
    let old = SourceLocation { side: DiffSide::Old, ..location("src/main.rs", 3)? };
    assert!(names.open_at(old, &session, &index, true));
    assert_eq!(card(&names)?.name, "Arc");
    let request = sent(&mut names)?;
    let content = main_content(&session)?;
    let expected = DefinitionRequest { at: location("src/main.rs", 4)?, column: 4, content };
    assert_eq!(request.request, LookupRequest::Definition(expected));

    let answer = Ok(ResolutionBuilder::new().external(3, STD, 4).answer());
    names.finish(reply(&request, answer), &mut session, &index);
    let content = card(&names)?.content(&index);
    assert_eq!(content.place.as_deref(), Some("std · alloc/src/util.rs:4"));
    assert_eq!(content.uses, "current version · 1 use in this diff");
    Ok(())
}

#[test]
fn a_removed_line_says_it_cannot_be_looked_up_and_matches_its_own_revision() -> Result<(), Box<dyn Error>> {
    let session = ReviewSession::new(revisions());
    let index = SymbolCache::default().index(&revisions());
    let mut names = NameLookup::default();
    assert!(names.open_at(
        SourceLocation { side: DiffSide::Old, ..location("src/main.rs", 2)? },
        &session,
        &index,
        true
    ));
    assert_eq!(names.take_lookups(), NONE);
    assert_eq!(card(&names)?.name, "Gone");
    let expected =
        "No definition of Gone in this diff; this line isn't in the current version, so it can't be looked up";
    assert_eq!(card(&names)?.content(&index).message.as_deref(), Some(expected));

    assert!(names.open_at(
        SourceLocation { side: DiffSide::Old, ..location("src/main.rs", 5)? },
        &session,
        &index,
        true
    ));
    assert_eq!(names.take_lookups(), NONE);
    let content = card(&names)?.content(&index);
    assert_eq!(content.code.map(|(_, code)| code), Some("fn helper(size: usize)"));
    assert!(names.open_at(location("src/main.rs", 6)?, &session, &index, false));
    assert_eq!(card(&names)?.content(&index).code.map(|(_, code)| code), Some("fn helper()"));
    Ok(())
}

const STD: &str = "std · alloc/src/util.rs";
const NONE: [Lookup; 0] = [];

fn helper(session: &ReviewSession, index: &SymbolIndex) -> Result<NameLookup, Box<dyn Error>> {
    let mut names = NameLookup::default();
    if !names.open_at(location("src/main.rs", 3)?, session, index, true) {
        return Err("expected the helper card".into());
    }
    Ok(names)
}

fn cache_card(session: &ReviewSession, index: &SymbolIndex) -> Result<(NameLookup, Lookup), Box<dyn Error>> {
    let mut names = NameLookup::default();
    if !names.open_at(location("src/main.rs", 2)?, session, index, true) || card(&names)?.name != "Cache" {
        return Err("expected the Cache card".into());
    }
    let request = sent(&mut names)?;
    Ok((names, request))
}

fn reopen(
    names: &mut NameLookup,
    session: &ReviewSession,
    index: &SymbolIndex,
    previous: &Lookup,
) -> Result<Lookup, Box<dyn Error>> {
    names.close();
    if !names.open_at(location("src/main.rs", 2)?, session, index, true) {
        return Err("expected the Cache card".into());
    }
    let retry = sent(names)?;
    if retry.request != previous.request {
        return Err("reopening should ask the same question".into());
    }
    Ok(retry)
}

fn sent(names: &mut NameLookup) -> Result<Lookup, Box<dyn Error>> {
    names.take_lookups().pop().ok_or_else(|| "a lookup".into())
}

fn card(names: &NameLookup) -> Result<&DefinitionCard, Box<dyn Error>> {
    names.card().ok_or_else(|| "an open card".into())
}

fn reply(lookup: &Lookup, result: Result<LookupAnswer, String>) -> LookupReply {
    LookupReply::new(lookup.clone(), result)
}

fn std_target() -> TargetLocation {
    TargetLocation { path: external(3, STD), side: DiffSide::New, line_number: 4 }
}

fn main_content(session: &ReviewSession) -> Result<Fingerprint, Box<dyn Error>> {
    Ok(session.document().source(&RepoPath::new("src/main.rs")?, DiffSide::New).ok_or("new source")?.content_id())
}

fn fixture() -> (ReviewSession, Arc<SymbolIndex>) {
    (ReviewSession::new(document()), index())
}

fn index() -> Arc<SymbolIndex> {
    SymbolCache::default().index(&document())
}

fn revisions() -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed("src/util.rs", "fn helper(size: usize) {}\n", "fn helper() {}\n")
        .changed(
            "src/main.rs",
            "fn main() {\n    Gone::run();\n    Arc::new(1);\n}\nfn run() { helper(1); }\n",
            "fn main() {\n    let one = 1;\n    tail();\n    Arc::new(1);\n}\nfn run() { helper(); }\n",
        )
        .build()
}

fn document() -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed("lib/util.rs", "", "fn helper() {}\n")
        .changed("src/a/util.rs", "", "fn helper() {}\nfn build() {}\n")
        .changed("src/main.rs", "", "fn main() {\n    let cache = Cache::build();\n    helper();\n    helper();\n\n}\n")
        .build()
}
