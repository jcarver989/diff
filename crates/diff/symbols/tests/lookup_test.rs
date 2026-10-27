use clankerdiff_core::{DiffSide, RepoPath, SourceLocation, testing::DocumentBuilder};
use clankerdiff_symbols::{Lookup, SymbolCache, SymbolIndex};
use std::{error::Error, sync::Arc};

#[test]
fn a_line_with_one_name_opens_its_card_directly() -> Result<(), Box<dyn Error>> {
    let index = index();
    let Some(Lookup::Card(card)) = Lookup::at(&index, location("src/main.rs", 3)?) else {
        return Err("expected a card".into());
    };
    assert_eq!(card.name, "helper");
    assert_eq!(card.target(&index), Some(location("src/a/util.rs", 1)?));
    Ok(())
}

#[test]
fn a_line_with_several_names_offers_one_hint_key_per_distinct_name() -> Result<(), Box<dyn Error>> {
    let index = index();
    let Some(Lookup::Hints(hints)) = Lookup::at(&index, location("src/main.rs", 2)?) else {
        return Err("expected hints".into());
    };
    let keyed: Vec<_> = hints.keyed().map(|(key, reference)| (key, reference.name.as_str())).collect();
    assert_eq!(keyed, [('a', "Cache"), ('s', "build")]);
    assert!(hints.contains("build"));
    let card = hints.pick('s').ok_or("s picks build")?;
    assert_eq!(card.name, "build");
    assert_eq!(card.origin, location("src/main.rs", 2)?);
    assert!(hints.pick('d').is_none());
    assert!(hints.pick('z').is_none());
    Ok(())
}

#[test]
fn a_line_without_names_has_nothing_to_look_up() -> Result<(), Box<dyn Error>> {
    assert!(Lookup::at(&index(), location("src/main.rs", 5)?).is_none());
    Ok(())
}

#[test]
fn cycling_walks_definitions_nearest_first_and_wraps() -> Result<(), Box<dyn Error>> {
    let index = index();
    let Some(Lookup::Card(mut card)) = Lookup::at(&index, location("src/main.rs", 3)?) else {
        return Err("expected a card".into());
    };
    let mut visited = Vec::new();
    for _ in 0..3 {
        let choice = card.choice(&index).ok_or("a definition")?;
        visited.push((choice.definition.location.path.as_str().to_owned(), choice.position, choice.count));
        assert!(card.cycle(&index));
    }
    assert_eq!(
        visited,
        [("src/a/util.rs".to_owned(), 0, 2), ("lib/util.rs".to_owned(), 1, 2), ("src/a/util.rs".to_owned(), 0, 2)]
    );
    Ok(())
}

#[test]
fn a_name_defined_once_or_not_at_all_does_not_cycle() -> Result<(), Box<dyn Error>> {
    let index = index();
    let Some(Lookup::Hints(hints)) = Lookup::at(&index, location("src/main.rs", 2)?) else {
        return Err("expected hints".into());
    };
    let mut build = hints.pick('s').ok_or("build")?;
    assert!(!build.cycle(&index));
    let mut cache = hints.pick('a').ok_or("Cache")?;
    assert!(!cache.cycle(&index));
    assert!(cache.choice(&index).is_none());
    assert!(cache.target(&index).is_none());
    Ok(())
}

fn index() -> Arc<SymbolIndex> {
    let document = DocumentBuilder::new()
        .changed("lib/util.rs", "", "fn helper() {}\n")
        .changed("src/a/util.rs", "", "fn helper() {}\nfn build() {}\n")
        .changed("src/main.rs", "", "fn main() {\n    let cache = Cache::build();\n    helper();\n    helper();\n\n}\n")
        .build();
    SymbolCache::default().index(&document)
}

fn location(path: &str, line_number: usize) -> Result<SourceLocation, Box<dyn Error>> {
    Ok(SourceLocation { path: RepoPath::new(path)?, side: DiffSide::New, line_number })
}
