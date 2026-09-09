use crate::{SyntaxError, language::resolve_language};
use arborium_highlight::{Injection, Span};
use arborium_tree_sitter::{
    InputEdit, Language, Node, Parser, Point, Query, QueryCursor, StreamingIterator, Tree,
};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::Arc,
};

/// Bytes handed to Tree-sitter per input callback. `parser_input_bytes` counts
/// these requests, so smaller chunks measure re-lexing more precisely.
const PARSER_CHUNK_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyntaxWorkStats {
    pub parser_input_bytes: usize,
    pub queried_bytes: usize,
    pub full_parses: usize,
    pub incremental_parses: usize,
    pub projected_bytes: usize,
    pub projected_lines: usize,
    pub reused_lines: usize,
    pub compared_nodes: usize,
}

#[derive(Default)]
pub(crate) struct Grammars(HashMap<String, Arc<Grammar>>);

/// Shared state for one append across a document and its injections.
pub(crate) struct AppendContext<'a> {
    pub grammars: &'a mut Grammars,
    pub stats: &'a mut SyntaxWorkStats,
    /// Line starts of the root stream, for Tree-sitter edit positions.
    pub line_starts: &'a [usize],
}

pub(crate) struct IncrementalDocument {
    parser: Option<Parser>,
    tree: Option<Tree>,
    grammar: Arc<Grammar>,
    length: usize,
    spans: BTreeMap<usize, Vec<Span>>,
    injections: BTreeMap<usize, Vec<InjectedDocument>>,
    boundaries: BTreeMap<usize, usize>,
}

impl Grammars {
    /// Creates an empty document, or `None` when no grammar is bundled.
    pub(crate) fn document(
        &mut self,
        language: &str,
    ) -> Result<Option<IncrementalDocument>, SyntaxError> {
        let grammar = if let Some(grammar) = self.0.get(language) {
            Arc::clone(grammar)
        } else {
            let Some((language_fn, highlights, injections)) = grammar_spec(language) else {
                return Ok(None);
            };
            let compile = |source| {
                Query::new(&language_fn, source).map_err(|source| SyntaxError::Query {
                    language: language.to_owned(),
                    source,
                })
            };
            let query_source = format!("{highlights}\n{injections}");
            let highlights = compile(highlights)?;
            let injections = compile(injections)?;
            let non_local = [&highlights, &injections].into_iter().any(|query| {
                (0..query.pattern_count()).any(|index| query.is_pattern_non_local(index))
            });
            let grammar = Arc::new(Grammar {
                highlights,
                injections,
                language: language_fn,
                non_local,
                query_source,
            });
            self.0.insert(language.to_owned(), Arc::clone(&grammar));
            grammar
        };
        Ok(Some(IncrementalDocument {
            parser: None,
            tree: None,
            grammar,
            length: 0,
            spans: BTreeMap::new(),
            injections: BTreeMap::new(),
            boundaries: BTreeMap::new(),
        }))
    }
}

impl IncrementalDocument {
    /// Appends the bytes of `source` beyond the previous length. `base` is the
    /// document's offset within the root stream.
    pub(crate) fn append(
        &mut self,
        source: &str,
        base: usize,
        depth: usize,
        cx: &mut AppendContext<'_>,
    ) -> Result<usize, SyntaxError> {
        if source.len() == self.length && self.tree.is_some() {
            return Ok(source.len());
        }
        let tree = self.parse_tree(source, base, cx)?;
        let mut start = if self.tree.is_none() || self.grammar.non_local {
            0
        } else {
            self.length
        };
        if let Some(previous) = &self.tree {
            for change in previous.changed_ranges(&tree) {
                let refined = if change.start_byte == 0 {
                    self.grammar
                        .error_prefix(previous.root_node(), tree.root_node(), cx.stats)
                } else {
                    None
                };
                start = start.min(refined.unwrap_or(change.start_byte));
            }
        }
        // A capture ending exactly at the edit point may be an unterminated
        // token that the appended text extends, so widen to it as well.
        loop {
            let earlier = self
                .boundaries
                .range(start..)
                .map(|(_, &begin)| begin)
                .min()
                .unwrap_or(start);
            if earlier >= start {
                break;
            }
            start = earlier;
        }
        let (spans, injections) = loop {
            let result = query(&self.grammar, &tree, source, start, cx.stats);
            let capture_start = result
                .0
                .iter()
                .map(|span| span.start as usize)
                .chain(result.1.iter().map(|injection| injection.start as usize))
                .min()
                .unwrap_or(start);
            if capture_start < start {
                start = capture_start;
            } else {
                break result;
            }
        };
        self.boundaries.split_off(&start.saturating_add(1));
        for (from, to) in spans.iter().map(|span| (span.start, span.end)).chain(
            injections
                .iter()
                .map(|injection| (injection.start, injection.end)),
        ) {
            self.boundaries
                .entry(to as usize)
                .and_modify(|begin| *begin = (*begin).min(from as usize))
                .or_insert(from as usize);
        }
        self.spans.split_off(&start);
        for span in spans {
            self.spans
                .entry(span.start as usize)
                .or_default()
                .push(span);
        }
        self.update_injections(source, base, start, injections, depth, cx)?;
        self.tree = Some(tree);
        self.length = source.len();
        Ok(start)
    }

    fn parse_tree(
        &mut self,
        source: &str,
        base: usize,
        cx: &mut AppendContext<'_>,
    ) -> Result<Tree, SyntaxError> {
        if self.parser.is_none() {
            let mut parser = Parser::new();
            parser.set_language(&self.grammar.language)?;
            self.parser = Some(parser);
        }
        if let Some(tree) = &mut self.tree {
            let old_end = end_point(cx.line_starts, base, self.length);
            tree.edit(&InputEdit {
                start_byte: self.length,
                old_end_byte: self.length,
                new_end_byte: source.len(),
                start_position: old_end,
                old_end_position: old_end,
                new_end_position: end_point(cx.line_starts, base, source.len()),
            });
            cx.stats.incremental_parses += 1;
        } else {
            cx.stats.full_parses += 1;
        }
        let bytes = source.as_bytes();
        let stats = &mut *cx.stats;
        self.parser
            .as_mut()
            .expect("initialized parser")
            .parse_with_options(
                &mut |offset, _| {
                    let end = offset.saturating_add(PARSER_CHUNK_BYTES).min(bytes.len());
                    let input = &bytes[offset.min(bytes.len())..end];
                    stats.parser_input_bytes += input.len();
                    input
                },
                self.tree.as_ref(),
                None,
            )
            .ok_or(SyntaxError::NoTree)
    }

    fn update_injections(
        &mut self,
        source: &str,
        base: usize,
        start: usize,
        injections: Vec<Injection>,
        depth: usize,
        cx: &mut AppendContext<'_>,
    ) -> Result<(), SyntaxError> {
        let mut previous_injections = self.injections.split_off(&start);
        if depth > 0 {
            for injection in injections {
                let from = injection.start as usize;
                let to = injection.end as usize;
                let Some(text) = source.get(from..to).filter(|text| !text.is_empty()) else {
                    continue;
                };
                let language = resolve_language(injection.language.as_str(), text)
                    .unwrap_or(&injection.language);
                let previous = previous_injections.get_mut(&from).and_then(|entries| {
                    entries
                        .iter()
                        .position(|entry| {
                            entry.language == language && text.len() >= entry.document.length
                        })
                        .map(|index| entries.swap_remove(index))
                });
                let mut entry = match previous {
                    Some(entry) => entry,
                    None => match cx.grammars.document(language)? {
                        Some(document) => InjectedDocument {
                            language: language.to_owned(),
                            document,
                        },
                        None => continue,
                    },
                };
                entry.document.append(text, base + from, depth - 1, cx)?;
                self.injections.entry(from).or_default().push(entry);
            }
        }
        Ok(())
    }

    pub(crate) fn spans_from(&self, start: usize) -> Vec<Span> {
        let mut result: Vec<_> = self
            .spans
            .range(start..)
            .flat_map(|(_, spans)| spans.iter().cloned())
            .collect();
        for (&offset, injections) in self.injections.range(start..) {
            for injection in injections {
                for mut span in injection.document.spans_from(0) {
                    span.start += u32::try_from(offset).expect("source limit fits u32");
                    span.end += u32::try_from(offset).expect("source limit fits u32");
                    result.push(span);
                }
            }
        }
        result
    }
}

impl Clone for IncrementalDocument {
    fn clone(&self) -> Self {
        Self {
            parser: None,
            tree: self.tree.clone(),
            grammar: Arc::clone(&self.grammar),
            length: self.length,
            spans: self.spans.clone(),
            injections: self.injections.clone(),
            boundaries: self.boundaries.clone(),
        }
    }
}

impl fmt::Debug for IncrementalDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IncrementalDocument")
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct InjectedDocument {
    language: String,
    document: IncrementalDocument,
}

struct Grammar {
    language: Language,
    highlights: Query,
    injections: Query,
    non_local: bool,
    query_source: String,
}

impl Grammar {
    fn error_prefix(
        &self,
        before: Node<'_>,
        after: Node<'_>,
        stats: &mut SyntaxWorkStats,
    ) -> Option<usize> {
        if self.non_local
            || self.query_source.contains("ERROR")
            || self.query_source.contains("(_ ")
            || self.query_source.contains("(_\n")
        {
            return None;
        }
        let before = self.error_root(before)?;
        let after = self.error_root(after)?;
        let mut end = before.start_byte().min(after.start_byte());
        let mut left = before.walk();
        let mut right = after.walk();
        for (before, after) in before.children(&mut left).zip(after.children(&mut right)) {
            if !same_subtree(before, after, stats) {
                break;
            }
            end = before.end_byte().min(after.end_byte());
        }
        Some(end)
    }

    fn error_root<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        if node.is_error() {
            Some(node)
        } else if !self.query_source.contains(node.kind()) && node.child_count() == 1 {
            node.child(0).filter(Node::is_error)
        } else {
            None
        }
    }
}

fn same_subtree(before: Node<'_>, after: Node<'_>, stats: &mut SyntaxWorkStats) -> bool {
    stats.compared_nodes += 1;
    if before.byte_range() != after.byte_range() || before.has_changes() || after.has_changes() {
        return false;
    }
    if before.id() == after.id() {
        return true;
    }
    if before.kind_id() != after.kind_id() || before.child_count() != after.child_count() {
        return false;
    }
    let mut left = before.walk();
    let mut right = after.walk();
    before
        .children(&mut left)
        .zip(after.children(&mut right))
        .enumerate()
        .all(|(index, (left, right))| {
            let Ok(index) = u32::try_from(index) else {
                return false;
            };
            before.field_name_for_child(index) == after.field_name_for_child(index)
                && same_subtree(left, right, stats)
        })
}

/// Row and column of byte `len` within a document that starts at `base` in
/// the root stream whose line starts are `line_starts`.
fn end_point(line_starts: &[usize], base: usize, len: usize) -> Point {
    let first = line_starts.partition_point(|&start| start <= base);
    let last = line_starts.partition_point(|&start| start <= base + len);
    let column = line_starts[first..last]
        .last()
        .map_or(len, |&start| base + len - start);
    Point::new(last - first, column)
}

fn query(
    grammar: &Grammar,
    tree: &Tree,
    source: &str,
    start: usize,
    stats: &mut SyntaxWorkStats,
) -> (Vec<Span>, Vec<Injection>) {
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(start..source.len());
    stats.queried_bytes += source.len() - start;
    let mut matches = cursor.matches(&grammar.highlights, tree.root_node(), source.as_bytes());
    let mut spans = Vec::new();
    while let Some(found) = matches.next() {
        for capture in found.captures {
            let name = grammar.highlights.capture_names()[capture.index as usize];
            if name.starts_with('_') || name.starts_with("injection.") {
                continue;
            }
            spans.push(Span {
                start: u32::try_from(capture.node.start_byte()).expect("source limit fits u32"),
                end: u32::try_from(capture.node.end_byte()).expect("source limit fits u32"),
                capture: name.to_owned(),
                pattern_index: u32::try_from(found.pattern_index).expect("query pattern fits u32"),
            });
        }
    }
    stats.queried_bytes += source.len() - start;
    let mut matches = cursor.matches(&grammar.injections, tree.root_node(), source.as_bytes());
    let mut injections = Vec::new();
    while let Some(found) = matches.next() {
        let mut content = None;
        let mut language = None;
        let mut include_children = false;
        for property in grammar.injections.property_settings(found.pattern_index) {
            match property.key.as_ref() {
                "injection.language" => language = property.value.as_deref().map(str::to_owned),
                "injection.include-children" => include_children = true,
                _ => {}
            }
        }
        for capture in found.captures {
            match grammar.injections.capture_names()[capture.index as usize] {
                "injection.content" => content = Some(capture.node),
                "injection.language" if language.is_none() => {
                    language = capture
                        .node
                        .utf8_text(source.as_bytes())
                        .ok()
                        .map(str::to_owned);
                }
                _ => {}
            }
        }
        if let (Some(node), Some(language)) = (content, language) {
            injections.push(Injection {
                start: u32::try_from(node.start_byte()).expect("source limit fits u32"),
                end: u32::try_from(node.end_byte()).expect("source limit fits u32"),
                language,
                include_children,
            });
        }
    }
    (spans, injections)
}

fn grammar_spec(language: &str) -> Option<(Language, &'static str, &'static str)> {
    macro_rules! grammar {
        ($module:ident) => {{
            use arborium::$module as grammar;
            (
                grammar::language().into(),
                &grammar::HIGHLIGHTS_QUERY,
                &grammar::INJECTIONS_QUERY,
            )
        }};
    }
    Some(match language {
        "asm" => grammar!(lang_asm),
        "bash" => grammar!(lang_bash),
        "batch" => grammar!(lang_batch),
        "c" => grammar!(lang_c),
        "c-sharp" => grammar!(lang_c_sharp),
        "clojure" => grammar!(lang_clojure),
        "cmake" => grammar!(lang_cmake),
        "commonlisp" => grammar!(lang_commonlisp),
        "cpp" => grammar!(lang_cpp),
        "css" => grammar!(lang_css),
        "dart" => grammar!(lang_dart),
        "diff" => grammar!(lang_diff),
        "dockerfile" => grammar!(lang_dockerfile),
        "elixir" => grammar!(lang_elixir),
        "erlang" => grammar!(lang_erlang),
        "fish" => grammar!(lang_fish),
        "go" => grammar!(lang_go),
        "graphql" => grammar!(lang_graphql),
        "haskell" => grammar!(lang_haskell),
        "hcl" => grammar!(lang_hcl),
        "html" => grammar!(lang_html),
        "ini" => grammar!(lang_ini),
        "java" => grammar!(lang_java),
        "javascript" => grammar!(lang_javascript),
        "json" => grammar!(lang_json),
        "just" => grammar!(lang_just),
        "kotlin" => grammar!(lang_kotlin),
        "lua" => grammar!(lang_lua),
        "make" => grammar!(lang_make),
        "markdown" => grammar!(lang_markdown),
        "meson" => grammar!(lang_meson),
        "ninja" => grammar!(lang_ninja),
        "nix" => grammar!(lang_nix),
        "objc" => grammar!(lang_objc),
        "ocaml" => grammar!(lang_ocaml),
        "perl" => grammar!(lang_perl),
        "php" => grammar!(lang_php),
        "powershell" => grammar!(lang_powershell),
        "proto" => grammar!(lang_proto),
        "python" => grammar!(lang_python),
        "r" => grammar!(lang_r),
        "rego" => grammar!(lang_rego),
        "ruby" => grammar!(lang_ruby),
        "rust" => grammar!(lang_rust),
        "scala" => grammar!(lang_scala),
        "scheme" => grammar!(lang_scheme),
        "scss" => grammar!(lang_scss),
        "solidity" => grammar!(lang_solidity),
        "sql" => grammar!(lang_sql),
        "starlark" => grammar!(lang_starlark),
        "svelte" => grammar!(lang_svelte),
        "swift" => grammar!(lang_swift),
        "toml" => grammar!(lang_toml),
        "tsx" => grammar!(lang_tsx),
        "typescript" => grammar!(lang_typescript),
        "vue" => grammar!(lang_vue),
        "x86asm" => grammar!(lang_x86asm),
        "xml" => grammar!(lang_xml),
        "yaml" => grammar!(lang_yaml),
        "zig" => grammar!(lang_zig),
        "zsh" => grammar!(lang_zsh),
        _ => return None,
    })
}
