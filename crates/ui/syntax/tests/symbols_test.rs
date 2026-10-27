use clankerdiff_syntax::{LanguageHint, LineSpan, SymbolExtractor, SymbolKind, SymbolTags, SyntaxError};
use std::fmt::Write;

#[test]
fn rust_definitions_carry_kinds_names_and_line_ranges() -> Result<(), SyntaxError> {
    let tags = extract(
        "src/lib.rs",
        "pub struct Cache {\n    entries: Vec<Entry>,\n}\n\nimpl Cache {\n    pub fn build() -> Self {\n        Self { entries: Vec::new() }\n    }\n}\n\npub trait Store {}\npub enum Mode { A }\nmod nested {}\nmacro_rules! square { () => {} }\nconst LIMIT: usize = 4;\nfn free() {}\n",
    )?;
    assert_eq!(
        definitions(&tags),
        [
            ("Cache", SymbolKind::Type, 0..3),
            ("Cache", SymbolKind::Implementation, 4..9),
            ("build", SymbolKind::Method, 5..8),
            ("Store", SymbolKind::Interface, 10..11),
            ("Mode", SymbolKind::Type, 11..12),
            ("nested", SymbolKind::Module, 12..13),
            ("square", SymbolKind::Macro, 13..14),
            ("LIMIT", SymbolKind::Constant, 14..15),
            ("free", SymbolKind::Function, 15..16),
        ]
    );
    let build = tags.definitions.iter().find(|definition| definition.name == "build").ok_or(SyntaxError::NoTree)?;
    assert_eq!(build.name_span, LineSpan { line: 5, columns: 11..16 });
    Ok(())
}

#[test]
fn rust_references_cover_calls_methods_paths_types_and_macros() -> Result<(), SyntaxError> {
    let tags = extract(
        "src/lib.rs",
        "fn run(cache: &Cache) {\n    let index = Index::build(cache);\n    index.lookup(\"x\");\n    helper();\n    println!(\"{index:?}\");\n    std::mem::drop(index);\n}\n",
    )?;
    assert_eq!(
        references(&tags),
        [("Cache", 0), ("Index", 1), ("build", 1), ("lookup", 2), ("helper", 3), ("println", 4), ("drop", 5),]
    );
    Ok(())
}

#[test]
fn a_definition_name_is_never_also_reported_as_a_reference() -> Result<(), SyntaxError> {
    let tags = extract("src/lib.rs", "struct Point;\nfn origin() -> Point { Point }\n")?;
    assert_eq!(references(&tags), [("Point", 1)]);
    assert_eq!(definitions(&tags).len(), 2);
    Ok(())
}

#[test]
fn enclosing_lists_every_definition_containing_a_line_outermost_first() -> Result<(), SyntaxError> {
    let tags = extract("src/lib.rs", "impl Cache {\n    fn build() {\n        work();\n    }\n}\n")?;
    let names: Vec<_> = tags.enclosing(2).map(|definition| (definition.name.as_str(), definition.kind)).collect();
    assert_eq!(names, [("Cache", SymbolKind::Implementation), ("build", SymbolKind::Method)]);
    assert_eq!(tags.references_on(2).iter().map(|reference| reference.name.as_str()).collect::<Vec<_>>(), ["work"]);
    Ok(())
}

#[test]
fn python_go_javascript_and_typescript_report_definitions_and_calls() -> Result<(), SyntaxError> {
    for (path, source, expected_definitions, expected_references) in [
        (
            "app.py",
            "class Cache:\n    def build(self):\n        return helper()\ndef helper():\n    pass\n",
            vec![("Cache", SymbolKind::Type), ("build", SymbolKind::Method), ("helper", SymbolKind::Function)],
            vec!["helper"],
        ),
        (
            "main.go",
            "package main\ntype Cache struct{}\nfunc (c Cache) Build() {}\nfunc main() { Cache{}.Build() }\n",
            vec![("Cache", SymbolKind::Type), ("Build", SymbolKind::Method), ("main", SymbolKind::Function)],
            vec!["Cache", "Cache", "Build"],
        ),
        (
            "app.js",
            "class Cache { build() { return helper(); } }\nfunction helper() {}\nconst run = () => new Cache().build();\n",
            vec![
                ("Cache", SymbolKind::Type),
                ("build", SymbolKind::Method),
                ("helper", SymbolKind::Function),
                ("run", SymbolKind::Function),
            ],
            vec!["helper", "Cache", "build"],
        ),
        (
            "app.ts",
            "interface Store {}\ntype Id = string;\nclass Cache implements Store { build(id: Id) { helper(); } }\nfunction helper() {}\n",
            vec![
                ("Store", SymbolKind::Interface),
                ("Id", SymbolKind::Type),
                ("Cache", SymbolKind::Type),
                ("build", SymbolKind::Method),
                ("helper", SymbolKind::Function),
            ],
            vec!["Store", "Id", "helper"],
        ),
        (
            "view.tsx",
            "function View() { return <div>{render()}</div>; }\n",
            vec![("View", SymbolKind::Function)],
            vec!["render"],
        ),
    ] {
        let tags = extract(path, source)?;
        let found: Vec<_> =
            tags.definitions.iter().map(|definition| (definition.name.as_str(), definition.kind)).collect();
        assert_eq!(found, expected_definitions, "{path}");
        let names: Vec<_> = tags.references.iter().map(|reference| reference.name.as_str()).collect();
        assert_eq!(names, expected_references, "{path}");
    }
    Ok(())
}

#[test]
fn languages_without_tags_queries_and_unknown_files_report_nothing() -> Result<(), SyntaxError> {
    for path in ["notes.md", "data.bin", "Cargo.toml"] {
        assert_eq!(extract(path, "fn main() {}\n")?, SymbolTags::default(), "{path}");
    }
    Ok(())
}

#[test]
fn an_extractor_reuses_compiled_queries_across_files() -> Result<(), SyntaxError> {
    let mut extractor = SymbolExtractor::new();
    for _ in 0..3 {
        let tags = extractor.extract(LanguageHint::Path("src/lib.rs"), "fn one() {}\n")?;
        assert_eq!(tags.definitions.len(), 1);
    }
    Ok(())
}

#[test]
fn rust_definitions_carry_their_doc_comments_and_signatures() -> Result<(), SyntaxError> {
    let tags = extract(
        "src/lib.rs",
        "// license header\n\n/// Parses source text.\n///\n/// Returns `Tags`.\n#[must_use]\npub fn parse(\n    source: &str,\n) -> Tags {\n    Tags::default()\n}\n\n/** Cached tags. */\npub struct Cache {\n    entries: Vec<Entry>,\n}\n\n// not documentation\nfn plain() {}\n\npub trait Store {\n    fn get(&self) -> u32;\n}\n",
    )?;
    assert_eq!(
        documented(&tags),
        [
            ("parse", "pub fn parse(\n    source: &str,\n) -> Tags", Some("Parses source text.\n\nReturns `Tags`.")),
            ("Cache", "pub struct Cache {\n    entries: Vec<Entry>,\n}", Some("Cached tags.")),
            ("plain", "fn plain()", None),
            ("Store", "pub trait Store", None),
            ("get", "fn get(&self) -> u32;", None),
        ]
    );
    Ok(())
}

#[test]
fn methods_dedent_their_signatures_and_long_ones_are_truncated() -> Result<(), SyntaxError> {
    let fields = (0..20).fold(String::new(), |mut fields, index| {
        let _ = writeln!(fields, "    field{index}: u8,");
        fields
    });
    let tags = extract(
        "src/lib.rs",
        &format!(
            "impl Cache {{\n    /// Builds it.\n    pub fn build(\n        size: usize,\n    ) -> Self {{\n        todo!()\n    }}\n}}\nstruct Wide {{\n{fields}}}\n"
        ),
    )?;
    let build = tags.definitions.iter().find(|definition| definition.name == "build").ok_or(SyntaxError::NoTree)?;
    assert_eq!(build.signature, "pub fn build(\n    size: usize,\n) -> Self");
    assert_eq!(build.docs.as_deref(), Some("Builds it."));
    let wide = tags.definitions.iter().find(|definition| definition.name == "Wide").ok_or(SyntaxError::NoTree)?;
    assert_eq!(wide.signature.lines().count(), 13);
    assert_eq!(wide.signature.lines().last(), Some("…"));
    Ok(())
}

#[test]
fn python_go_and_typescript_docs_follow_their_conventions() -> Result<(), SyntaxError> {
    for (path, source, name, signature, docs) in [
        (
            "app.py",
            "@cached\ndef build(size):\n    \"\"\"Builds a cache.\n\n    Sized by `size`.\n    \"\"\"\n    return size\n",
            "build",
            "def build(size):",
            Some("Builds a cache.\n\nSized by `size`."),
        ),
        ("app.py", "# a comment\ndef plain():\n    return 1\n", "plain", "def plain():", None),
        (
            "main.go",
            "package main\n\n// Cache holds entries.\ntype Cache struct {\n\tentries []int\n}\n",
            "Cache",
            "Cache struct {\n\tentries []int\n}",
            Some("Cache holds entries."),
        ),
        (
            "main.go",
            "package main\n\n// Build makes one.\nfunc Build() int {\n\treturn 1\n}\n",
            "Build",
            "func Build() int",
            Some("Build makes one."),
        ),
        (
            "app.ts",
            "/**\n * Runs the job.\n * @param id the job\n */\nexport function run(id: string): void {\n  go(id);\n}\n",
            "run",
            "function run(id: string): void",
            Some("Runs the job.\n@param id the job"),
        ),
        (
            "app.ts",
            "// plain comment\nexport const go = (id: string) => {\n  run(id);\n};\n",
            "go",
            "go = (id: string) =>",
            None,
        ),
    ] {
        let tags = extract(path, source)?;
        let definition =
            tags.definitions.iter().find(|definition| definition.name == name).ok_or(SyntaxError::NoTree)?;
        assert_eq!(definition.signature, signature, "{path} {name}");
        assert_eq!(definition.docs.as_deref(), docs, "{path} {name}");
    }
    Ok(())
}

#[test]
fn rustdoc_links_link_definitions_and_hidden_example_lines_are_tidied() -> Result<(), SyntaxError> {
    let tags = extract(
        "src/lib.rs",
        "/// Shared pointer. See [`Rc`], [Weak] and [`clone`][clone], or [the book](https://doc.rust-lang.org).\n/// Indexing `a[0]` and x[i] stay put.\n///\n/// ```\n/// # use std::sync::Arc;\n/// let shared = Arc::new(5);\n/// #\n/// ```\n///\n/// ```text\n/// # kept\n/// ```\n///\n/// [clone]: Arc::clone\npub struct Arc;\n",
    )?;
    assert_eq!(
        documented(&tags)[0].2,
        Some(
            "Shared pointer. See `Rc`, Weak and `clone`, or [the book](https://doc.rust-lang.org).\nIndexing `a[0]` and x[i] stay put.\n\n```\nlet shared = Arc::new(5);\n```\n\n```text\n# kept\n```"
        )
    );
    Ok(())
}

fn extract(path: &str, source: &str) -> Result<SymbolTags, SyntaxError> {
    SymbolExtractor::new().extract(LanguageHint::Path(path), source)
}

fn definitions(tags: &SymbolTags) -> Vec<(&str, SymbolKind, std::ops::Range<usize>)> {
    tags.definitions
        .iter()
        .map(|definition| (definition.name.as_str(), definition.kind, definition.lines.clone()))
        .collect()
}

fn references(tags: &SymbolTags) -> Vec<(&str, usize)> {
    tags.references.iter().map(|reference| (reference.name.as_str(), reference.span.line)).collect()
}

fn documented(tags: &SymbolTags) -> Vec<(&str, &str, Option<&str>)> {
    tags.definitions
        .iter()
        .map(|definition| (definition.name.as_str(), definition.signature.as_str(), definition.docs.as_deref()))
        .collect()
}
