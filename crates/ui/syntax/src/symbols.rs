use crate::{LanguageHint, SyntaxError, incremental::grammar_language, resolve_language};
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::Arc,
};
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, QueryMatch, StreamingIterator};

const SIGNATURE_LINES: usize = 12;
const BODY_KINDS: [&str; 5] = ["block", "statement_block", "declaration_list", "class_body", "compound_statement"];
const RUSTDOC_FENCE_ATTRIBUTES: [&str; 6] =
    ["ignore", "no_run", "should_panic", "compile_fail", "test_harness", "standalone_crate"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Type,
    Interface,
    Module,
    Constant,
    Macro,
    Implementation,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LineSpan {
    pub line: usize,
    pub columns: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDefinition {
    pub name: String,
    pub kind: SymbolKind,
    pub name_span: LineSpan,
    pub lines: Range<usize>,
    pub signature: String,
    pub docs: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolReference {
    pub name: String,
    pub span: LineSpan,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolTags {
    pub definitions: Vec<SymbolDefinition>,
    pub references: Vec<SymbolReference>,
}

impl SymbolTags {
    pub fn enclosing(&self, line: usize) -> impl Iterator<Item = &SymbolDefinition> {
        self.definitions.iter().filter(move |definition| definition.lines.contains(&line))
    }

    #[must_use]
    pub fn references_on(&self, line: usize) -> &[SymbolReference] {
        let start = self.references.partition_point(|reference| reference.span.line < line);
        let end = self.references.partition_point(|reference| reference.span.line <= line);
        &self.references[start..end]
    }
}

#[derive(Debug, Clone, Default)]
pub struct SymbolExtractor {
    grammars: HashMap<&'static str, Option<Arc<TagsGrammar>>>,
}

impl SymbolExtractor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extract<'a>(&mut self, hint: impl Into<LanguageHint<'a>>, source: &str) -> Result<SymbolTags, SyntaxError> {
        let Some(language) = resolve_language(hint, source) else {
            return Ok(SymbolTags::default());
        };
        let Some(grammar) = self.grammar(language)? else {
            return Ok(SymbolTags::default());
        };
        grammar.extract(source)
    }

    fn grammar(&mut self, language: &'static str) -> Result<Option<Arc<TagsGrammar>>, SyntaxError> {
        if let Some(grammar) = self.grammars.get(language) {
            return Ok(grammar.clone());
        }
        let grammar = TagsGrammar::new(language)?.map(Arc::new);
        self.grammars.insert(language, grammar.clone());
        Ok(grammar)
    }
}

fn tags_query(language: &str) -> Option<&'static str> {
    Some(match language {
        "go" => include_str!("../queries/tags/go.scm"),
        "javascript" => {
            concat!(include_str!("../queries/tags/javascript.scm"), include_str!("../queries/tags/jsdoc.scm"))
        }
        "python" => include_str!("../queries/tags/python.scm"),
        "rust" => include_str!("../queries/tags/rust.scm"),
        "tsx" | "typescript" => {
            concat!(include_str!("../queries/tags/typescript.scm"), include_str!("../queries/tags/jsdoc.scm"))
        }
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy)]
enum Role {
    Definition(SymbolKind),
    Reference,
}

#[derive(Debug, Clone, Copy)]
enum Capture {
    Name,
    Doc,
    Documented,
    Role(Role),
    Other,
}

impl Capture {
    fn parse(name: &str) -> Self {
        match name {
            "name" => Self::Name,
            "doc" => Self::Doc,
            "documented" => Self::Documented,
            _ if name.starts_with("reference.") => Self::Role(Role::Reference),
            _ => {
                name.strip_prefix("definition.").map_or(Self::Other, |kind| Self::Role(Role::Definition(kind_of(kind))))
            }
        }
    }
}

fn kind_of(kind: &str) -> SymbolKind {
    match kind {
        "function" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "interface" => SymbolKind::Interface,
        "module" => SymbolKind::Module,
        "constant" => SymbolKind::Constant,
        "macro" => SymbolKind::Macro,
        "implementation" => SymbolKind::Implementation,
        _ => SymbolKind::Type,
    }
}

#[derive(Debug)]
struct TagsGrammar {
    name: &'static str,
    language: Language,
    query: Query,
    captures: Vec<Capture>,
}

impl TagsGrammar {
    fn new(language: &'static str) -> Result<Option<Self>, SyntaxError> {
        let (Some(source), Some(grammar)) = (tags_query(language), grammar_language(language)) else {
            return Ok(None);
        };
        let query = Query::new(&grammar, source)
            .map_err(|source| SyntaxError::Query { language: language.to_owned(), source })?;
        let captures = query.capture_names().iter().map(|name| Capture::parse(name)).collect();
        Ok(Some(Self { name: language, language: grammar, query, captures }))
    }

    fn extract(&self, source: &str) -> Result<SymbolTags, SyntaxError> {
        let mut parser = Parser::new();
        parser.set_language(&self.language)?;
        let tree = parser.parse(source, None).ok_or(SyntaxError::NoTree)?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, tree.root_node(), source.as_bytes());
        let mut collector = TagCollector::new(source);
        while let Some(found) = matches.next() {
            collector.add(found.pattern_index, self.read_match(found));
        }
        Ok(collector.finish(self.name == "rust"))
    }

    fn read_match<'tree>(&self, found: &QueryMatch<'_, 'tree>) -> MatchedTag<'tree> {
        let mut tag = MatchedTag::default();
        for capture in found.captures() {
            match self.captures[capture.index as usize] {
                Capture::Name => tag.name = Some(capture.node),
                Capture::Doc => tag.comments.push(capture.node),
                Capture::Documented => tag.documented = Some(capture.node),
                Capture::Role(role) => tag.role = Some((role, capture.node)),
                Capture::Other => {}
            }
        }
        tag
    }
}

#[derive(Default)]
struct MatchedTag<'tree> {
    name: Option<Node<'tree>>,
    role: Option<(Role, Node<'tree>)>,
    documented: Option<Node<'tree>>,
    comments: Vec<Node<'tree>>,
}

struct PendingDefinition<'tree> {
    pattern: usize,
    item: Node<'tree>,
    definition: SymbolDefinition,
}

type NameKey = (usize, usize);

struct TagCollector<'a, 'tree> {
    source: &'a str,
    definitions: BTreeMap<NameKey, PendingDefinition<'tree>>,
    references: BTreeMap<NameKey, SymbolReference>,
    docs: HashMap<usize, BTreeMap<usize, Node<'tree>>>,
}

impl<'a, 'tree> TagCollector<'a, 'tree> {
    fn new(source: &'a str) -> Self {
        Self { source, definitions: BTreeMap::new(), references: BTreeMap::new(), docs: HashMap::new() }
    }

    fn add(&mut self, pattern: usize, tag: MatchedTag<'tree>) {
        if let Some(item) = tag.documented {
            let comments = tag.comments.into_iter().map(|comment| (comment.start_byte(), comment));
            self.docs.entry(item.id()).or_default().extend(comments);
        }
        let (Some(name), Some((role, item))) = (tag.name, tag.role) else {
            return;
        };
        let Some(text) = symbol_text(self.source, name) else {
            return;
        };
        match role {
            Role::Definition(kind) => self.add_definition(pattern, kind, name, text, item),
            Role::Reference => self.add_reference(name, text),
        }
    }

    fn add_definition(&mut self, pattern: usize, kind: SymbolKind, name: Node<'tree>, text: &str, item: Node<'tree>) {
        let key = name_key(name);
        if self.definitions.get(&key).is_some_and(|existing| existing.pattern <= pattern) {
            return;
        }
        let definition = SymbolDefinition {
            name: text.to_owned(),
            kind,
            name_span: span(name),
            lines: lines(item),
            signature: signature(item, self.source),
            docs: None,
        };
        self.definitions.insert(key, PendingDefinition { pattern, item, definition });
    }

    fn add_reference(&mut self, name: Node<'tree>, text: &str) {
        self.references
            .entry(name_key(name))
            .or_insert_with(|| SymbolReference { name: text.to_owned(), span: span(name) });
    }

    fn finish(self, rust: bool) -> SymbolTags {
        let Self { source, definitions, mut references, docs } = self;
        references.retain(|key, _| !definitions.contains_key(key));
        let definitions = definitions
            .into_values()
            .map(|PendingDefinition { item, definition, .. }| SymbolDefinition {
                docs: docs.get(&item.id()).and_then(|comments| render_docs(item, comments, source, rust)),
                ..definition
            })
            .collect();
        SymbolTags { definitions, references: references.into_values().collect() }
    }
}

fn symbol_text<'a>(source: &'a str, name: Node<'_>) -> Option<&'a str> {
    source.get(name.byte_range()).filter(|text| !text.is_empty() && !text.contains('\n'))
}

fn name_key(name: Node<'_>) -> NameKey {
    (name.start_byte(), name.end_byte())
}

fn signature(item: Node<'_>, source: &str) -> String {
    let mut cursor = item.walk();
    let start = item.children(&mut cursor).find(|child| !is_preamble(*child)).unwrap_or(item);
    let end =
        body(item).filter(|body| BODY_KINDS.contains(&body.kind())).map_or(item.end_byte(), |body| body.start_byte());
    let text = source.get(start.start_byte()..end).unwrap_or_default().trim_end();
    let line_start = source[..start.start_byte()].rfind('\n').map_or(0, |index| index + 1);
    let indent = source[line_start..].bytes().take_while(|byte| matches!(byte, b' ' | b'\t')).count();
    let mut lines: Vec<&str> = text
        .lines()
        .enumerate()
        .map(|(index, line)| if index == 0 { line } else { strip_indent(line, indent) })
        .collect();
    if lines.len() > SIGNATURE_LINES {
        lines.truncate(SIGNATURE_LINES);
        lines.push("…");
    }
    lines.join("\n")
}

fn is_preamble(node: Node<'_>) -> bool {
    matches!(node.kind(), "attributes" | "decorator" | "comment" | "line_comment" | "block_comment")
}

fn body(item: Node<'_>) -> Option<Node<'_>> {
    item.child_by_field_name("body").or_else(|| item.child_by_field_name("value")?.child_by_field_name("body"))
}

fn strip_indent(line: &str, indent: usize) -> &str {
    let skip = line.bytes().take(indent).take_while(|byte| matches!(byte, b' ' | b'\t')).count();
    &line[skip..]
}

fn render_docs(item: Node<'_>, comments: &BTreeMap<usize, Node<'_>>, source: &str, rust: bool) -> Option<String> {
    let mut blocks: Vec<String> = comments
        .values()
        .rev()
        .take_while(|comment| {
            comment.start_byte() >= item.start_byte()
                || comment.next_sibling().is_none_or(|next| comment.end_position().row + 1 >= next.start_position().row)
        })
        .map(|comment| clean_comment(source.get(comment.byte_range()).unwrap_or_default()))
        .collect();
    blocks.reverse();
    let docs = tidy_docs(&blocks.join("\n"), rust);
    (!docs.is_empty()).then_some(docs)
}

fn tidy_docs(docs: &str, rust: bool) -> String {
    let mut lines = Vec::new();
    let mut fence: Option<(&str, bool)> = None;
    for line in docs.lines() {
        let trimmed = line.trim_start();
        if let Some((marker, hides_lines)) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            } else if hides_lines && (trimmed == "#" || trimmed.starts_with("# ")) {
                continue;
            }
            lines.push(line.to_owned());
        } else if let Some(marker) = fence_marker(trimmed) {
            fence = Some((marker, rust && is_rust_fence(&trimmed[marker.len()..])));
            lines.push(line.to_owned());
        } else if !is_link_definition(trimmed) {
            lines.push(unwrap_doc_links(line));
        }
    }
    lines.join("\n").trim().to_owned()
}

fn fence_marker(line: &str) -> Option<&str> {
    let ticks = line.len() - line.trim_start_matches('`').len();
    let tildes = line.len() - line.trim_start_matches('~').len();
    let length = ticks.max(tildes);
    (length >= 3).then(|| &line[..length])
}

fn is_rust_fence(info: &str) -> bool {
    info.split([',', ' '])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .all(|token| token == "rust" || token.starts_with("edition") || RUSTDOC_FENCE_ATTRIBUTES.contains(&token))
}

fn is_link_definition(line: &str) -> bool {
    line.strip_prefix('[')
        .and_then(|rest| rest.split_once("]:"))
        .is_some_and(|(label, target)| !label.is_empty() && !label.contains(']') && !target.trim().is_empty())
}

fn unwrap_doc_links(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(index) = rest.find(['`', '[']) {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        if rest.starts_with('`') {
            let span = code_span_len(rest);
            out.push_str(&rest[..span]);
            rest = &rest[span..];
            continue;
        }
        let follows_word = out.chars().next_back().is_some_and(|last| last.is_alphanumeric() || last == '_');
        if let Some((text, consumed)) = doc_link(rest).filter(|_| !follows_word) {
            out.push_str(text);
            rest = &rest[consumed..];
        } else {
            out.push('[');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn code_span_len(text: &str) -> usize {
    let ticks = text.len() - text.trim_start_matches('`').len();
    text[ticks..].find(&text[..ticks]).map_or(ticks, |end| ticks + end + ticks)
}

fn doc_link(text: &str) -> Option<(&str, usize)> {
    let close = bracket_end(text)?;
    let label = &text[1..close];
    let after = &text[close + 1..];
    if after.starts_with('(') {
        return None;
    }
    if after.starts_with('[') {
        let reference = bracket_end(after)?;
        return Some((label, close + 1 + reference + 1));
    }
    let code = label.len() > 1 && label.starts_with('`') && label.ends_with('`');
    let path = label.chars().next().is_some_and(char::is_alphabetic)
        && label.chars().all(|character| character.is_alphanumeric() || "_:.<>!()".contains(character));
    (code || path).then_some((label, close + 1))
}

fn bracket_end(text: &str) -> Option<usize> {
    let mut index = 1;
    while let Some(character) = text[index..].chars().next() {
        match character {
            ']' => return Some(index),
            '[' => return None,
            '`' => index += code_span_len(&text[index..]),
            _ => index += character.len_utf8(),
        }
    }
    None
}

fn clean_comment(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(line) = trimmed.strip_prefix("//") {
        let line = line.trim_start_matches('/');
        return line.strip_prefix(' ').unwrap_or(line).to_owned();
    }
    let block = trimmed
        .strip_prefix("/**")
        .or_else(|| trimmed.strip_prefix("/*"))
        .map(|body| body.strip_suffix("*/").unwrap_or(body));
    let body = block.unwrap_or(text);
    let indent = body
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    body.lines()
        .enumerate()
        .map(|(index, line)| match block {
            Some(_) => {
                let line = line.trim();
                line.strip_prefix('*').map_or(line, |rest| rest.strip_prefix(' ').unwrap_or(rest))
            }
            None if index == 0 => line.trim(),
            None => line.get(indent..).unwrap_or_default().trim_end(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn span(node: Node<'_>) -> LineSpan {
    let start = node.start_position();
    LineSpan { line: start.row, columns: start.column..start.column + node.byte_range().len() }
}

fn lines(node: Node<'_>) -> Range<usize> {
    node.start_position().row..node.end_position().row + 1
}
