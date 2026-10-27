use crate::{Definition, SymbolIndex, SymbolReference};
use clankerdiff_core::SourceLocation;
use std::ops::Range;

pub const HINT_KEYS: [char; 9] = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Hints(NameHints),
    Card(DefinitionCard),
}

impl Lookup {
    #[must_use]
    pub fn at(index: &SymbolIndex, origin: SourceLocation) -> Option<Self> {
        let mut names: Vec<SymbolReference> = Vec::new();
        for reference in index.references_at(&origin) {
            if !names.iter().any(|existing| existing.name == reference.name) {
                names.push(reference.clone());
            }
        }
        names.truncate(HINT_KEYS.len());
        match names.as_slice() {
            [] => None,
            [reference] => Some(Self::Card(DefinitionCard::new(origin, reference))),
            _ => Some(Self::Hints(NameHints { origin, names })),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameHints {
    pub origin: SourceLocation,
    pub names: Vec<SymbolReference>,
}

impl NameHints {
    pub fn keyed(&self) -> impl Iterator<Item = (char, &SymbolReference)> {
        HINT_KEYS.into_iter().zip(&self.names)
    }

    #[must_use]
    pub fn pick(&self, key: char) -> Option<DefinitionCard> {
        let index = HINT_KEYS.iter().position(|hint| *hint == key)?;
        Some(DefinitionCard::new(self.origin.clone(), self.names.get(index)?))
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|reference| reference.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionCard {
    pub origin: SourceLocation,
    pub name: String,
    pub columns: Range<usize>,
    choice: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice<'a> {
    pub definition: &'a Definition,
    pub position: usize,
    pub count: usize,
}

impl DefinitionCard {
    #[must_use]
    pub fn new(origin: SourceLocation, reference: &SymbolReference) -> Self {
        Self { origin, name: reference.name.clone(), columns: reference.span.columns.clone(), choice: 0 }
    }

    #[must_use]
    pub fn shows(&self, origin: &SourceLocation, columns: &Range<usize>) -> bool {
        &self.origin == origin && &self.columns == columns
    }

    #[must_use]
    pub fn definitions<'a>(&self, index: &'a SymbolIndex) -> Vec<&'a Definition> {
        index.definitions(&self.name, Some(&self.origin))
    }

    #[must_use]
    pub fn choice<'a>(&self, index: &'a SymbolIndex) -> Option<Choice<'a>> {
        let definitions = self.definitions(index);
        let position = if self.choice < definitions.len() { self.choice } else { 0 };
        Some(Choice { definition: definitions.get(position)?, position, count: definitions.len() })
    }

    pub fn cycle(&mut self, index: &SymbolIndex) -> bool {
        let count = self.definitions(index).len();
        if count < 2 {
            return false;
        }
        self.choice = (self.choice + 1) % count;
        true
    }

    #[must_use]
    pub fn target(&self, index: &SymbolIndex) -> Option<SourceLocation> {
        self.choice(index).map(|choice| choice.definition.location.clone())
    }
}
