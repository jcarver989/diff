use crate::{SymbolIndex, SymbolReference};
use clankerdiff_core::{
    DefinitionRequest, DefinitionTarget, Lookup, LookupReply, LookupRequest, RepoPath, Resolution, ReviewSession,
    SourceLocation, TargetLocation,
};
use std::{collections::HashMap, mem, ops::Range};

pub const HINT_KEYS: [char; 9] = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamePick {
    Hints(NameHints),
    Card(DefinitionCard),
}

impl NamePick {
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

    #[must_use]
    pub const fn origin(&self) -> &SourceLocation {
        match self {
            Self::Hints(hints) => &hints.origin,
            Self::Card(card) => &card.origin,
        }
    }

    fn marks(&self, references: &[SymbolReference]) -> Vec<(Range<usize>, Mark)> {
        let (hints, card) = match self {
            Self::Hints(hints) => (Some(hints), None),
            Self::Card(card) => (None, Some(card)),
        };
        references
            .iter()
            .filter_map(|reference| {
                let mark = if hints.is_some_and(|hints| hints.contains(&reference.name)) {
                    Mark::Hinted
                } else if card.is_some_and(|card| card.columns == reference.span.columns) {
                    Mark::Card
                } else {
                    return None;
                };
                Some((reference.span.columns.clone(), mark))
            })
            .collect()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Hinted,
    Card,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionCard {
    pub origin: SourceLocation,
    pub name: String,
    pub columns: Range<usize>,
    choice: usize,
    cycled: bool,
    following: bool,
    answer: Answer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Answer {
    Local,
    Pending,
    Resolved { targets: Vec<DefinitionTarget>, hover: Option<String> },
    MatchedByName,
}

struct Candidate<'a> {
    target: TargetLocation,
    place: String,
    code: Option<(RepoPath, &'a str)>,
    docs: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardContent<'a> {
    pub message: Option<String>,
    pub code: Option<(RepoPath, &'a str)>,
    pub docs: Option<&'a str>,
    pub place: Option<String>,
    pub uses: String,
    pub can_jump: bool,
    pub can_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settled {
    Ignored,
    Redraw,
    Opened,
    Jump(TargetLocation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activation {
    Idle,
    Wait,
    Jump(TargetLocation),
}

impl DefinitionCard {
    #[must_use]
    pub fn new(origin: SourceLocation, reference: &SymbolReference) -> Self {
        Self {
            origin,
            name: reference.name.clone(),
            columns: reference.span.columns.clone(),
            choice: 0,
            cycled: false,
            following: false,
            answer: Answer::Local,
        }
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.answer, Answer::Pending)
    }

    fn follow(&mut self) -> bool {
        self.following = self.is_pending();
        self.following
    }

    #[must_use]
    pub fn shows(&self, origin: &SourceLocation, columns: &Range<usize>) -> bool {
        &self.origin == origin && &self.columns == columns
    }

    #[must_use]
    pub fn content<'a>(&'a self, index: &'a SymbolIndex) -> CardContent<'a> {
        let uses = format!("{} in this diff", uses_label(index.uses(&self.name).len()));
        let uses = match self.answer {
            Answer::Pending => format!("looking up · {uses}"),
            Answer::MatchedByName => format!("matched by name · {uses}"),
            Answer::Local | Answer::Resolved { .. } => uses,
        };
        let mut candidates = self.candidates(index);
        let count = candidates.len();
        if count == 0 {
            let pending = self.is_pending();
            let message = if pending {
                format!("Looking up {}…", self.name)
            } else {
                format!("No definition of {} in this diff", self.name)
            };
            return CardContent {
                message: Some(message),
                code: None,
                docs: None,
                place: None,
                uses,
                can_jump: pending,
                can_cycle: false,
            };
        }
        let position = self.position(count);
        let candidate = candidates.swap_remove(position);
        CardContent {
            message: None,
            code: candidate.code,
            docs: candidate.docs,
            place: Some(counted(candidate.place, position, count)),
            uses,
            can_jump: true,
            can_cycle: count > 1,
        }
    }

    fn cycle(&mut self, index: &SymbolIndex) -> bool {
        let count = self.candidates(index).len();
        if count < 2 {
            return false;
        }
        self.choice = (self.position(count) + 1) % count;
        self.cycled = true;
        true
    }

    #[must_use]
    pub fn target(&self, index: &SymbolIndex) -> Option<TargetLocation> {
        let mut candidates = self.candidates(index);
        let position = self.position(candidates.len());
        (position < candidates.len()).then(|| candidates.swap_remove(position).target)
    }

    const fn position(&self, count: usize) -> usize {
        if self.choice < count { self.choice } else { 0 }
    }

    fn candidates<'a>(&'a self, index: &'a SymbolIndex) -> Vec<Candidate<'a>> {
        if let Answer::Resolved { targets, hover } = &self.answer {
            return targets
                .iter()
                .map(|target| Candidate {
                    target: target.location.clone(),
                    place: target.location.to_string(),
                    code: target
                        .preview
                        .as_deref()
                        .filter(|_| hover.is_none())
                        .map(|preview| (target.location.path.display_path(), preview)),
                    docs: hover.as_deref(),
                })
                .collect();
        }
        index
            .definitions(&self.name, Some(&self.origin))
            .into_iter()
            .map(|definition| {
                let location = &definition.location;
                Candidate {
                    target: location.clone().into(),
                    place: format!("{}:{}", location.path, location.line_number),
                    code: Some((location.path.clone(), definition.signature.as_str())),
                    docs: definition.docs.as_deref(),
                }
            })
            .collect()
    }

    fn request(&self, session: &ReviewSession) -> Option<DefinitionRequest> {
        if session.shows_external(&self.origin.path) {
            return None;
        }
        DefinitionRequest::at(session.presentation().document(), self.origin.clone(), self.columns.start)
    }

    fn answers(&self, request: &DefinitionRequest) -> bool {
        request.at == self.origin && request.column == self.columns.start
    }

    fn resolve(&mut self, resolution: &Resolution) {
        if matches!(self.answer, Answer::Resolved { .. }) {
            return;
        }
        self.answer = if resolution.targets.is_empty() || self.cycled {
            Answer::MatchedByName
        } else {
            self.choice = 0;
            Answer::Resolved { targets: resolution.targets.clone(), hover: resolution.hover.clone() }
        };
    }

    fn settle(&self, index: &SymbolIndex) -> Settled {
        match self.target(index) {
            Some(target) if self.following => Settled::Jump(target),
            _ => Settled::Redraw,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NameLookup {
    pick: Option<NamePick>,
    cache: LookupCache,
    outgoing: Vec<Lookup>,
}

impl NameLookup {
    #[must_use]
    pub const fn pick(&self) -> Option<&NamePick> {
        self.pick.as_ref()
    }

    #[must_use]
    pub const fn hints(&self) -> Option<&NameHints> {
        match &self.pick {
            Some(NamePick::Hints(hints)) => Some(hints),
            _ => None,
        }
    }

    #[must_use]
    pub const fn card(&self) -> Option<&DefinitionCard> {
        match &self.pick {
            Some(NamePick::Card(card)) => Some(card),
            _ => None,
        }
    }

    pub fn open_at(
        &mut self,
        origin: SourceLocation,
        session: &ReviewSession,
        index: &SymbolIndex,
        enabled: bool,
    ) -> bool {
        match NamePick::at(index, origin) {
            Some(NamePick::Card(card)) => self.show(card, session, enabled),
            Some(hints) => self.pick = Some(hints),
            None => return false,
        }
        true
    }

    pub fn show(&mut self, mut card: DefinitionCard, session: &ReviewSession, enabled: bool) {
        if enabled {
            self.outgoing.extend(self.cache.show(&mut card, session));
        }
        self.pick = Some(NamePick::Card(card));
    }

    pub fn pick_hint(&mut self, key: char, session: &ReviewSession, enabled: bool) -> bool {
        let Some(card) = self.hints().and_then(|hints| hints.pick(key)) else {
            return false;
        };
        self.show(card, session, enabled);
        true
    }

    pub fn cycle(&mut self, index: &SymbolIndex) -> bool {
        self.card_mut().is_some_and(|card| card.cycle(index))
    }

    pub fn activate(&mut self, index: &SymbolIndex) -> Activation {
        let Some(card) = self.card_mut() else {
            return Activation::Idle;
        };
        if card.follow() {
            return Activation::Wait;
        }
        card.target(index).map_or(Activation::Idle, Activation::Jump)
    }

    pub fn finish(&mut self, reply: LookupReply, session: &mut ReviewSession, index: &SymbolIndex) -> Settled {
        let card = match &mut self.pick {
            Some(NamePick::Card(card)) => Some(card),
            _ => None,
        };
        self.cache.finish(reply, session, card, index)
    }

    pub fn navigated(&mut self, session: &mut ReviewSession, enabled: bool) {
        self.pick = None;
        if session.pending_reference().is_some() {
            self.outgoing.extend(self.cache.source(session, enabled));
        }
    }

    pub fn document_changed(&mut self, session: &mut ReviewSession, enabled: bool) {
        self.pick = None;
        self.cache.clear();
        self.outgoing.extend(self.cache.source(session, enabled));
    }

    pub fn close(&mut self) -> bool {
        self.pick.take().is_some()
    }

    pub fn take_lookups(&mut self) -> Vec<Lookup> {
        mem::take(&mut self.outgoing)
    }

    #[must_use]
    pub fn marks(&self, location: &SourceLocation, references: &[SymbolReference]) -> Vec<(Range<usize>, Mark)> {
        match &self.pick {
            Some(pick) if pick.origin() == location => pick.marks(references),
            _ => Vec::new(),
        }
    }

    const fn card_mut(&mut self) -> Option<&mut DefinitionCard> {
        match &mut self.pick {
            Some(NamePick::Card(card)) => Some(card),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct LookupCache {
    next_token: u64,
    answers: HashMap<DefinitionRequest, Resolution>,
    waiting: HashMap<LookupRequest, u64>,
}

impl LookupCache {
    fn show(&mut self, card: &mut DefinitionCard, session: &ReviewSession) -> Option<Lookup> {
        let request = card.request(session)?;
        if let Some(resolution) = self.answers.get(&request) {
            card.resolve(resolution);
            return None;
        }
        card.answer = Answer::Pending;
        let request = LookupRequest::Definition(request);
        (!self.waiting.contains_key(&request)).then(|| self.issue(request))
    }

    fn clear(&mut self) {
        self.answers.clear();
        self.waiting.clear();
    }

    fn finish(
        &mut self,
        reply: LookupReply,
        session: &mut ReviewSession,
        card: Option<&mut DefinitionCard>,
        index: &SymbolIndex,
    ) -> Settled {
        match reply {
            LookupReply::Source { token, path, result } => {
                if !self.settles(&LookupRequest::Source(path.clone()), token) {
                    return Settled::Ignored;
                }
                match result {
                    Ok(document) => {
                        if session.open_reference(&path, document) {
                            Settled::Opened
                        } else {
                            Settled::Redraw
                        }
                    }
                    Err(error) => {
                        session.reference_failed(&path, error);
                        Settled::Redraw
                    }
                }
            }
            LookupReply::Definition { token, request, resolution } => {
                if !self.settles(&LookupRequest::Definition(request.clone()), token) {
                    return Settled::Ignored;
                }
                let mut card = card.filter(|card| card.answers(&request));
                if let Some(card) = card.as_deref_mut() {
                    card.resolve(&resolution);
                }
                if !resolution.targets.is_empty() {
                    self.answers.insert(request, resolution);
                }
                card.map_or(Settled::Ignored, |card| card.settle(index))
            }
        }
    }

    fn source(&mut self, session: &mut ReviewSession, enabled: bool) -> Option<Lookup> {
        let source = session.reference_source()?.clone();
        if !enabled {
            session.reference_failed(&source, "definitions outside the diff need a connected repository");
            return None;
        }
        Some(self.issue(LookupRequest::Source(source)))
    }

    fn issue(&mut self, request: LookupRequest) -> Lookup {
        let token = self.next_token;
        self.next_token += 1;
        self.waiting.insert(request.clone(), token);
        Lookup { token, request }
    }

    fn settles(&mut self, request: &LookupRequest, token: u64) -> bool {
        let current = self.waiting.get(request) == Some(&token);
        if current {
            self.waiting.remove(request);
        }
        current
    }
}

fn uses_label(count: usize) -> String {
    if count == 1 { "1 use".to_owned() } else { format!("{count} uses") }
}

fn counted(location: String, position: usize, count: usize) -> String {
    match count {
        0 | 1 => location,
        count => format!("{location} · {} of {count}", position + 1),
    }
}
