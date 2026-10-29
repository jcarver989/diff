use clankerdiff_core::{DiffDocument, DiffSide, FileDiff, Fingerprint, RepoPath, SourceLocation};
use clankerdiff_syntax::{LanguageHint, SymbolDefinition, SymbolExtractor, SymbolKind, SymbolReference, SymbolTags};
use std::{
    cmp::Reverse,
    collections::{BTreeSet, HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolChange {
    Added,
    Changed,
    Removed,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub kind: SymbolKind,
    pub container: Option<String>,
    pub location: SourceLocation,
    pub lines: Range<usize>,
    pub change: SymbolChange,
    pub signature: String,
    pub docs: Option<String>,
}

impl Definition {
    #[must_use]
    pub fn label(&self) -> String {
        match &self.container {
            Some(container) => format!("{container}::{}", self.name),
            None => self.name.clone(),
        }
    }

    #[must_use]
    pub fn contains(&self, location: &SourceLocation) -> bool {
        self.location.path == location.path
            && self.location.side == location.side
            && self.lines.contains(&location.line_number)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolUse {
    pub location: SourceLocation,
    pub columns: Range<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    files: Vec<FileSymbols>,
    definitions: Vec<Definition>,
    by_name: HashMap<String, Vec<usize>>,
}

#[derive(Debug, Clone)]
struct FileSymbols {
    path: RepoPath,
    old: Arc<SymbolTags>,
    new: Arc<SymbolTags>,
    removed: BTreeSet<usize>,
}

impl FileSymbols {
    fn tags(&self, side: DiffSide) -> &SymbolTags {
        match side {
            DiffSide::Old => &self.old,
            DiffSide::New => &self.new,
        }
    }
}

impl SymbolIndex {
    fn build(document: &DiffDocument, cache: &mut SymbolCache) -> Self {
        let mut index = Self::default();
        let mut used = BTreeSet::new();
        for file in &document.files {
            let mut tags = |side| {
                file.source_document(side).map_or_else(Arc::default, |source| {
                    let key = (source.content_id(), file.path.clone());
                    used.insert(key.clone());
                    cache.tags(key, file.path.as_str(), source.text())
                })
            };
            let symbols = FileSymbols {
                path: file.path.clone(),
                old: tags(DiffSide::Old),
                new: tags(DiffSide::New),
                removed: file.changed_lines(DiffSide::Old),
            };
            index.add_file(file, &symbols);
            index.files.push(symbols);
        }
        cache.retain(&used);
        index
    }

    fn add_file(&mut self, file: &FileDiff, symbols: &FileSymbols) {
        let added = file.changed_lines(DiffSide::New);
        let key = |tags: &SymbolTags, definition: &SymbolDefinition| {
            (definition.name.clone(), definition.kind, container(tags, definition).map(|found| found.name.clone()))
        };
        let old: HashMap<_, _> =
            symbols.old.definitions.iter().map(|definition| (key(&symbols.old, definition), definition)).collect();
        let mut matched = HashSet::new();
        for definition in &symbols.new.definitions {
            let identity = key(&symbols.new, definition);
            let counterpart = old.get(&identity);
            if counterpart.is_some() {
                matched.insert(identity.clone());
            }
            let change = match counterpart {
                None => SymbolChange::Added,
                Some(previous) if touches(definition, &added) || touches(previous, &symbols.removed) => {
                    SymbolChange::Changed
                }
                Some(_) => SymbolChange::Unchanged,
            };
            self.push(file, DiffSide::New, identity.2, definition, change);
        }
        for definition in &symbols.old.definitions {
            let identity = key(&symbols.old, definition);
            let change = if matched.contains(&identity) { SymbolChange::Unchanged } else { SymbolChange::Removed };
            self.push(file, DiffSide::Old, identity.2, definition, change);
        }
    }

    fn push(
        &mut self,
        file: &FileDiff,
        side: DiffSide,
        container: Option<String>,
        definition: &SymbolDefinition,
        change: SymbolChange,
    ) {
        if definition.kind != SymbolKind::Implementation {
            self.by_name.entry(definition.name.clone()).or_default().push(self.definitions.len());
        }
        self.definitions.push(Definition {
            name: definition.name.clone(),
            kind: definition.kind,
            container,
            location: SourceLocation { path: file.path.clone(), side, line_number: definition.name_span.line + 1 },
            lines: definition.lines.start + 1..definition.lines.end + 1,
            change,
            signature: definition.signature.clone(),
            docs: definition.docs.clone(),
        });
    }

    #[must_use]
    pub fn definitions(&self, name: &str, origin: Option<&SourceLocation>) -> Vec<&Definition> {
        let side = origin.map_or(DiffSide::New, |origin| origin.side);
        let only_other_side = match side {
            DiffSide::New => SymbolChange::Removed,
            DiffSide::Old => SymbolChange::Added,
        };
        let mut found: Vec<&Definition> = self
            .by_name
            .get(name)
            .into_iter()
            .flatten()
            .map(|&index| &self.definitions[index])
            .filter(|definition| definition.location.side == side || definition.change == only_other_side)
            .collect();
        if let Some(origin) = origin {
            found.sort_by_key(|definition| Reverse(proximity(&origin.path, &definition.location.path)));
        }
        found
    }

    #[must_use]
    pub fn file_definitions(&self, path: &RepoPath, side: DiffSide) -> Vec<&Definition> {
        self.definitions
            .iter()
            .filter(|definition| {
                &definition.location.path == path
                    && definition.location.side == side
                    && definition.kind != SymbolKind::Implementation
            })
            .collect()
    }

    #[must_use]
    pub fn path_names(&self, location: &SourceLocation) -> Vec<String> {
        let mut path: Vec<&Definition> =
            self.definitions.iter().filter(|definition| definition.contains(location)).collect();
        path.sort_by_key(|definition| (definition.lines.start, Reverse(definition.lines.end)));
        let mut names: Vec<String> = path.into_iter().map(|definition| definition.name.clone()).collect();
        names.dedup();
        names
    }

    #[must_use]
    pub fn references_at(&self, location: &SourceLocation) -> &[SymbolReference] {
        self.file(&location.path)
            .map_or(&[], |file| file.tags(location.side).references_on(location.line_number.saturating_sub(1)))
    }

    #[must_use]
    pub fn uses(&self, name: &str) -> Vec<SymbolUse> {
        let mut uses = Vec::new();
        for file in &self.files {
            for (side, tags) in [(DiffSide::New, &file.new), (DiffSide::Old, &file.old)] {
                for reference in tags.references.iter().filter(|reference| reference.name == name) {
                    let line_number = reference.span.line + 1;
                    if side == DiffSide::Old && !file.removed.contains(&line_number) {
                        continue;
                    }
                    uses.push(SymbolUse {
                        location: SourceLocation { path: file.path.clone(), side, line_number },
                        columns: reference.span.columns.clone(),
                    });
                }
            }
        }
        uses
    }

    fn file(&self, path: &RepoPath) -> Option<&FileSymbols> {
        self.files.iter().find(|file| &file.path == path)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SymbolCache {
    extractor: SymbolExtractor,
    tags: HashMap<(Fingerprint, RepoPath), Arc<SymbolTags>>,
    current: Option<(Arc<DiffDocument>, Arc<SymbolIndex>)>,
}

impl SymbolCache {
    pub fn index(&mut self, document: &Arc<DiffDocument>) -> Arc<SymbolIndex> {
        if let Some((indexed, index)) = &self.current
            && Arc::ptr_eq(indexed, document)
        {
            return Arc::clone(index);
        }
        let index = Arc::new(SymbolIndex::build(document, self));
        self.current = Some((Arc::clone(document), Arc::clone(&index)));
        index
    }

    fn tags(&mut self, key: (Fingerprint, RepoPath), path: &str, text: &str) -> Arc<SymbolTags> {
        if let Some(tags) = self.tags.get(&key) {
            return Arc::clone(tags);
        }
        let tags = Arc::new(self.extractor.extract(LanguageHint::Path(path), text).unwrap_or_default());
        self.tags.insert(key, Arc::clone(&tags));
        tags
    }

    fn retain(&mut self, used: &BTreeSet<(Fingerprint, RepoPath)>) {
        self.tags.retain(|key, _| used.contains(key));
    }
}

fn container<'a>(tags: &'a SymbolTags, definition: &SymbolDefinition) -> Option<&'a SymbolDefinition> {
    tags.enclosing(definition.name_span.line)
        .filter(|candidate| *candidate != definition && candidate.lines != definition.lines)
        .max_by_key(|candidate| candidate.lines.start)
}

fn touches(definition: &SymbolDefinition, lines: &BTreeSet<usize>) -> bool {
    lines.range(definition.lines.start + 1..=definition.lines.end).next().is_some()
}

fn proximity(origin: &RepoPath, path: &RepoPath) -> usize {
    origin.as_str().split('/').zip(path.as_str().split('/')).take_while(|(left, right)| left == right).count()
}
