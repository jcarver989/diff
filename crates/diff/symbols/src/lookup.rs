use crate::{SymbolIndex, SymbolReference};
use clankerdiff_core::{
    DefinitionRequest, DefinitionTarget, DiffSide, Lookup, LookupReply, LookupRequest, LookupStatus, LookupUnavailable,
    RepoPath, Resolution, ReviewSession, SourceLocation, TargetLocation,
};
use std::{collections::HashMap, mem, ops::Range};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Hold {
    Hover,
    #[default]
    Pinned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionCard {
    pub origin: SourceLocation,
    pub name: String,
    pub columns: Range<usize>,
    choice: usize,
    cycled: bool,
    following: bool,
    request: Option<DefinitionRequest>,
    answer: Answer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Answer {
    Local(Option<LookupUnavailable>),
    Pending,
    Semantic(Resolution),
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
            request: None,
            answer: Answer::Local(None),
        }
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.answer, Answer::Pending)
    }

    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(&self.answer, Answer::Semantic(Resolution { status: LookupStatus::Failed(_), .. }))
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
        let mut candidates = self.candidates(index);
        let count = candidates.len();
        let uses = format!("{} in this diff", uses_label(index.uses(&self.name).len()));
        let uses = self.label(count).map_or_else(|| uses.clone(), |label| format!("{label} · {uses}"));
        if count == 0 {
            let docs = self.hover();
            return CardContent {
                message: self.empty_message(docs.is_some()),
                code: None,
                docs,
                place: None,
                uses,
                can_jump: self.is_pending() || self.is_failed(),
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

    fn label(&self, count: usize) -> Option<&'static str> {
        match &self.answer {
            Answer::Pending => Some("looking up"),
            Answer::Semantic(resolution) if self.semantic_targets().is_some() => {
                self.current_version(resolution).then_some("current version")
            }
            Answer::Semantic(_) if count > 0 => Some("matched by name"),
            Answer::Semantic(resolution) if resolution.hover.is_some() => {
                Some(if self.current_version(resolution) { "current version docs" } else { "docs only" })
            }
            Answer::Local(_) | Answer::Semantic(_) => None,
        }
    }

    fn current_version(&self, resolution: &Resolution) -> bool {
        resolution.current_version || self.origin.side == DiffSide::Old
    }

    fn empty_message(&self, documented: bool) -> Option<String> {
        let name = &self.name;
        let not_in_diff = |reason: &LookupUnavailable| format!("No definition of {name} in this diff; {reason}");
        match &self.answer {
            Answer::Pending => Some(format!("Looking up {name}…")),
            Answer::Local(None) => Some(format!("No definition of {name} in this diff")),
            Answer::Local(Some(reason)) => Some(not_in_diff(reason)),
            Answer::Semantic(resolution) => match &resolution.status {
                LookupStatus::Complete if documented => None,
                LookupStatus::Complete => Some(format!("No definition of {name} found")),
                LookupStatus::Unavailable(reason) => Some(not_in_diff(reason)),
                LookupStatus::Failed(failure) => {
                    Some(format!("Couldn't look up {name}: {failure}. Go to definition to retry"))
                }
            },
        }
    }

    fn cycle(&mut self, index: &SymbolIndex) -> bool {
        let count = self.candidates(index).len();
        if count < 2 {
            return false;
        }
        self.choice = (self.position(count) + 1) % count;
        self.cycled |= self.semantic_targets().is_none();
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

    const fn semantic(&self) -> Option<&Resolution> {
        match &self.answer {
            Answer::Semantic(resolution) => Some(resolution),
            Answer::Local(_) | Answer::Pending => None,
        }
    }

    fn hover(&self) -> Option<&str> {
        self.semantic()?.hover.as_deref()
    }

    fn semantic_targets(&self) -> Option<&[DefinitionTarget]> {
        let targets = &self.semantic()?.targets;
        (!self.cycled && !targets.is_empty()).then_some(targets.as_slice())
    }

    fn candidates<'a>(&'a self, index: &'a SymbolIndex) -> Vec<Candidate<'a>> {
        if let Some(targets) = self.semantic_targets() {
            let hover = self.hover();
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
                    docs: hover,
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

    fn request(&self, session: &ReviewSession) -> Result<DefinitionRequest, Option<LookupUnavailable>> {
        if session.shows_external(&self.origin.path) {
            return Err(None);
        }
        DefinitionRequest::at(session.presentation().document(), self.origin.clone(), self.columns.start).map_err(Some)
    }

    fn answers(&self, request: &DefinitionRequest) -> bool {
        self.request.as_ref() == Some(request)
    }

    fn resolve(&mut self, resolution: &Resolution) {
        if self.semantic().is_some() {
            return;
        }
        self.answer = Answer::Semantic(resolution.clone());
        if self.semantic_targets().is_some() {
            self.choice = 0;
        }
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
    card: Option<DefinitionCard>,
    hover: CardHover,
    cache: LookupCache,
    outgoing: Vec<Lookup>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CardHover {
    hold: Hold,
    token_hovered: bool,
    card_hovered: bool,
}

impl NameLookup {
    #[must_use]
    pub const fn card(&self) -> Option<&DefinitionCard> {
        self.card.as_ref()
    }

    #[must_use]
    pub fn card_columns(&self, location: &SourceLocation) -> Option<Range<usize>> {
        self.card.as_ref().filter(|card| &card.origin == location).map(|card| card.columns.clone())
    }

    pub fn open_at(
        &mut self,
        origin: SourceLocation,
        session: &ReviewSession,
        index: &SymbolIndex,
        enabled: bool,
    ) -> bool {
        let mut names: Vec<&SymbolReference> = Vec::new();
        for reference in index.references_at(&origin) {
            if !names.iter().any(|existing| existing.name == reference.name) {
                names.push(reference);
            }
        }
        let shown = self
            .card
            .as_ref()
            .filter(|card| card.origin == origin)
            .and_then(|card| names.iter().position(|reference| reference.name == card.name));
        let next = shown.map_or(0, |position| (position + 1) % names.len().max(1));
        let Some(reference) = names.get(next) else {
            return false;
        };
        let card = DefinitionCard::new(origin, reference);
        self.show(card, session, enabled, Hold::Pinned);
        true
    }

    pub fn show(&mut self, mut card: DefinitionCard, session: &ReviewSession, enabled: bool, hold: Hold) {
        if enabled {
            self.outgoing.extend(self.cache.show(&mut card, session));
        }
        self.card = Some(card);
        self.hover = CardHover { hold, token_hovered: hold == Hold::Hover, card_hovered: false };
    }

    pub const fn pin(&mut self) {
        self.hover.hold = Hold::Pinned;
    }

    pub const fn hover_token(&mut self, hovered: bool) {
        self.hover.token_hovered = hovered;
    }

    pub const fn hover_card(&mut self, hovered: bool) {
        self.hover.card_hovered = hovered;
    }

    #[must_use]
    pub fn lingering(&self) -> bool {
        self.card.is_some() && self.hover.hold == Hold::Hover && !self.hover.token_hovered && !self.hover.card_hovered
    }

    pub fn cycle(&mut self, index: &SymbolIndex) -> bool {
        let cycled = self.card.as_mut().is_some_and(|card| card.cycle(index));
        if cycled {
            self.pin();
        }
        cycled
    }

    pub fn activate(&mut self, session: &ReviewSession, index: &SymbolIndex, enabled: bool) -> Activation {
        let Some(card) = self.card.as_mut() else {
            return Activation::Idle;
        };
        if enabled && card.is_failed() && card.target(index).is_none() {
            card.answer = Answer::Local(None);
            self.outgoing.extend(self.cache.show(card, session));
        }
        if card.follow() {
            self.hover.hold = Hold::Pinned;
            return Activation::Wait;
        }
        card.target(index).map_or(Activation::Idle, Activation::Jump)
    }

    pub fn finish(&mut self, reply: LookupReply, session: &mut ReviewSession, index: &SymbolIndex) -> Settled {
        self.cache.finish(reply, session, self.card.as_mut(), index)
    }

    pub fn navigated(&mut self, session: &mut ReviewSession, enabled: bool) {
        self.card = None;
        if session.pending_reference().is_some() {
            self.outgoing.extend(self.cache.source(session, enabled));
        }
    }

    pub fn document_changed(&mut self, session: &mut ReviewSession, enabled: bool) {
        self.card = None;
        self.cache.clear();
        self.outgoing.extend(self.cache.source(session, enabled));
    }

    pub fn close(&mut self) -> bool {
        self.card.take().is_some()
    }

    pub fn take_lookups(&mut self) -> Vec<Lookup> {
        mem::take(&mut self.outgoing)
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
        let request = match card.request(session) {
            Ok(request) => request,
            Err(reason) => {
                card.answer = Answer::Local(reason);
                return None;
            }
        };
        card.request = Some(request.clone());
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
                if settled(&resolution) {
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

fn settled(resolution: &Resolution) -> bool {
    match resolution.status {
        LookupStatus::Complete => !resolution.targets.is_empty() || resolution.hover.is_some(),
        LookupStatus::Unavailable(_) => true,
        LookupStatus::Failed(_) => false,
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
