# clankerdiff-syntax

Reusable syntax highlighting via Tree-sitter and Arborium. Native and browser WASM builds use the same parser runtime.

`ThemedHighlighter::append` accepts exact UTF-8 text, including partial lines, without
adding newlines. Complete input is simply the first append. `highlight_document`
is a bounded cache around the same operation: it accepts a stable source `Fingerprint`,
a language hint, and a source-producing closure evaluated only on a cache miss.
Every use of a fingerprint must supply the same exact source, including line endings
and the presence of a trailing newline. Reuse precomputed content identities when
available. Line-sequence identities are valid only with a consistent text encoding
and a separate identity domain from exact documents and single cells. Source assembly
from line-oriented models belongs in the closure, so cache hits do not join lines.
Both operations expose `DocumentHighlights` with per-line byte spans; there are no
separate source-span or line-sequence highlighting engines.

`SyntaxStream` retains its complete source and Tree-sitter context. Appends edit the
previous tree, re-query affected structural/capture regions, and reuse unchanged
line projections through a persistent vector. Injected languages retain separate
trees, with a three-level injection limit. Non-local queries and retroactive syntax
changes can invalidate earlier content; stream updates are not a promise that
earlier lines will never change.

Empty updates on the same theme do no parsing or highlighting. Changing the theme
recolors retained captures without reparsing, including cached complete documents.
Cloned streams can append independently. Unknown-language streams reuse preceding
line projections without parsing.

`CacheConfig::max_documents` bounds retained cached documents (zero disables the
cache). `max_source_bytes` bounds each source, for both cached documents and streams.

`SyntaxStream::work_stats()` reports cumulative parser callback bytes, query range
bytes, full/incremental parse counts, projected bytes/lines, reused lines, and
structural nodes compared while refining error-wrapper invalidation.
`HighlightStats::bytes` counts parser callback bytes for both cached documents and
streams, including repeated requests and injected languages, not just input length.
Query range bytes count each range submitted to each highlight/injection query,
not a claim about Tree-sitter's internal node visits. Appends re-query from the
earliest capture or changed range the edit touches, so query and projection work
can reuse the unaffected prefix even inside long open constructs. When a parser
only changes an unqueried root error wrapper, unchanged child subtrees provide
a more precise boundary; their comparison work is reported in `compared_nodes`.
Non-local queries re-query the whole document. Tree-sitter itself may still
re-lex large regions for invalid text or indentation-sensitive grammars, which
`parser_input_bytes` reports honestly.

Input limits are checked before changing source, highlights, revisions, or work
counters. `SyntaxError` distinguishes input limits, missing grammars, query
compilation, language setup, and missing parse results. Underlying query and language
errors are retained as error sources. On parser/query failure, source and published
highlights remain unchanged, and the next attempt rebuilds the discarded parser state.

## Example

```rust
use clankerdiff_syntax::{Fingerprint, SyntaxError, SyntaxHighlighter, SyntaxStream};
use clankerdiff_theme::SyntaxTheme;

fn example() -> Result<(), SyntaxError> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut stream = SyntaxStream::new("rust");
    let update = highlighter.with_theme(&theme)
        .append(&mut stream, "/* open\nclosed */")?;
    assert_eq!(update.highlights.line_count(), 2);
    assert_eq!(update.changed_lines, 0..2);

    let text = "fn main() {}\n";
    let document = highlighter.with_theme(&theme).highlight_document(
        Fingerprint::of([text]), "rust", || text,
    )?;
    assert_eq!(document.line_count(), 1);
    Ok(())
}
```
