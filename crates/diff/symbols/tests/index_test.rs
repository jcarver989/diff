use clankerdiff_core::{DiffSide, RepoPath, SourceLocation, testing::DocumentBuilder};
use clankerdiff_symbols::{SymbolCache, SymbolChange, SymbolIndex, SymbolKind};
use std::{error::Error, sync::Arc};

#[test]
fn definitions_report_kind_container_location_and_how_the_change_touched_them() {
    let index = index();
    let summary = |name: &str| {
        index
            .definitions(name, None)
            .into_iter()
            .map(|definition| {
                (
                    definition.kind,
                    definition.container.clone(),
                    definition.location.side,
                    definition.location.line_number,
                    definition.change,
                )
            })
            .collect::<Vec<_>>()
    };
    let cache = Some("Cache".to_owned());
    assert_eq!(summary("build"), [(SymbolKind::Method, cache.clone(), DiffSide::New, 6, SymbolChange::Changed)]);
    assert_eq!(summary("fresh"), [(SymbolKind::Method, cache.clone(), DiffSide::New, 12, SymbolChange::Added)]);
    assert_eq!(summary("stale"), [(SymbolKind::Method, cache.clone(), DiffSide::Old, 10, SymbolChange::Removed)]);
    assert_eq!(summary("len"), [(SymbolKind::Method, cache, DiffSide::New, 16, SymbolChange::Unchanged)]);
    assert_eq!(summary("Cache"), [(SymbolKind::Type, None, DiffSide::New, 1, SymbolChange::Unchanged)]);
    assert_eq!(summary("main"), [(SymbolKind::Function, None, DiffSide::New, 1, SymbolChange::Added)]);
    assert_eq!(summary("missing").len(), 0);
    let build = index.definitions("build", None);
    assert_eq!(build[0].lines, 6..11);
    assert_eq!(build[0].label(), "Cache::build");
}

#[test]
fn definitions_nearest_the_origin_rank_first_and_carry_signatures_and_docs() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .changed("lib/a.rs", "", "fn helper() {}\n")
        .changed("src/c.rs", "", "/// Nearby.\npub fn helper(size: usize) -> u32 {\n    0\n}\n")
        .changed("src/b/y.rs", "", "fn helper() {}\n")
        .changed("src/b/x.rs", "", "fn helper() {}\nfn run() { helper(); }\n")
        .build();
    let index = SymbolCache::default().index(&document);
    let found = index.definitions("helper", Some(&location("src/b/x.rs", DiffSide::New, 2)?));
    let paths: Vec<_> = found.iter().map(|definition| definition.location.path.as_str()).collect();
    assert_eq!(paths, ["src/b/x.rs", "src/b/y.rs", "src/c.rs", "lib/a.rs"]);
    assert_eq!(found[2].signature, "pub fn helper(size: usize) -> u32");
    assert_eq!(found[2].docs.as_deref(), Some("Nearby."));
    Ok(())
}

#[test]
fn path_at_lists_enclosing_definitions_outermost_first() -> Result<(), Box<dyn Error>> {
    let index = index();
    let path = index.path_at(&location("src/lib.rs", DiffSide::New, 8)?);
    let names: Vec<_> = path.iter().map(|definition| (definition.name.as_str(), definition.kind)).collect();
    assert_eq!(names, [("Cache", SymbolKind::Implementation), ("build", SymbolKind::Method)]);
    assert_eq!(index.path_at(&location("src/lib.rs", DiffSide::New, 4)?).len(), 0);
    Ok(())
}

#[test]
fn file_definitions_skip_implementation_blocks() -> Result<(), Box<dyn Error>> {
    let index = index();
    let names: Vec<_> = index
        .file_definitions(&RepoPath::new("src/lib.rs")?, DiffSide::New)
        .iter()
        .map(|definition| definition.name.as_str())
        .collect();
    assert_eq!(names, ["Cache", "build", "fresh", "len"]);
    Ok(())
}

#[test]
fn references_and_uses_point_at_names_in_changed_sources() -> Result<(), Box<dyn Error>> {
    let index = index();
    let names: Vec<_> = index
        .references_at(&location("src/main.rs", DiffSide::New, 2)?)
        .iter()
        .map(|reference| reference.name.as_str())
        .collect();
    assert_eq!(names, ["Cache", "build"]);
    let uses = index.uses("fresh");
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].location, location("src/lib.rs", DiffSide::New, 8)?);
    assert_eq!(uses[0].columns, 14..19);
    Ok(())
}

#[test]
fn uses_on_the_old_side_only_count_removed_lines() {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "fn a() { helper(); }\nfn b() { helper(); }\n", "fn a() { helper(); }\n")
        .build();
    let index = SymbolCache::default().index(&document);
    let sides: Vec<_> =
        index.uses("helper").iter().map(|found| (found.location.side, found.location.line_number)).collect();
    assert_eq!(sides, [(DiffSide::New, 1), (DiffSide::Old, 2)]);
}

#[test]
fn files_without_sources_or_grammars_contribute_nothing() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new().changed("notes.md", "# a\n", "# b\n").binary("image.png").build();
    let index = SymbolCache::default().index(&document);
    assert_eq!(index.file_definitions(&RepoPath::new("notes.md")?, DiffSide::New).len(), 0);
    assert_eq!(index.uses("a").len(), 0);
    Ok(())
}

const OLD_LIB: &str = "pub struct Cache {
    entries: Vec<u32>,
}

impl Cache {
    pub fn build() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn stale(&self) -> bool {
        false
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
";

const NEW_LIB: &str = "pub struct Cache {
    entries: Vec<u32>,
}

impl Cache {
    pub fn build() -> Self {
        let cache = Self { entries: Vec::new() };
        cache.fresh();
        cache
    }

    pub fn fresh(&self) -> bool {
        true
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
";

fn index() -> Arc<SymbolIndex> {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", OLD_LIB, NEW_LIB)
        .changed("src/main.rs", "", "fn main() {\n    let cache = Cache::build();\n    cache.len();\n}\n")
        .build();
    SymbolCache::default().index(&document)
}

fn location(path: &str, side: DiffSide, line_number: usize) -> Result<SourceLocation, Box<dyn Error>> {
    Ok(SourceLocation { path: RepoPath::new(path)?, side, line_number })
}

#[test]
fn the_cache_reuses_an_index_until_the_document_changes() {
    let document = DocumentBuilder::new().changed("src/lib.rs", "", "fn one() {}\n").build();
    let mut cache = SymbolCache::default();
    let first = cache.index(&document);
    assert!(Arc::ptr_eq(&first, &cache.index(&document)));
    let next = DocumentBuilder::new().changed("src/lib.rs", "", "fn one() {}\nfn two() {}\n").build();
    let second = cache.index(&next);
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(second.definitions("two", None).len(), 1);
}
