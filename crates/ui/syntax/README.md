# clankerdiff-syntax

Reusable syntax highlighting via Tree-sitter and Arborium. Native and browser WASM builds use the same parser runtime.

`SyntaxStream::append` accepts exact UTF-8 text, including partial lines, without
adding newlines. Complete input is simply the first append: `analyze` parses a
complete source in one append and extracts its symbol tags from the same tree, so
highlights and symbols never cost two parses. It exposes `DocumentHighlights`, whose
`line` returns a borrowed `HighlightLine` of byte spans; there are no separate
source-span or line-sequence highlighting engines. Nothing is cached here: the owner of an
immutable source holds a `SyntaxCell` beside it that analyzes it on first use and
is shared by every clone of its owner, so syntax lives exactly as long as the source.

Highlight spans carry a theme-independent `HighlightId`. Renderers resolve it with
`SyntaxTheme::highlight`, an indexed lookup, so a theme change restyles retained
highlights without reparsing. Grammars and queries are compiled once per process
and shared by every stream.

`SyntaxStream` retains its complete source and Tree-sitter context. Appends edit the
previous tree, re-query affected structural/capture regions, and rebuild highlights
from the first affected line. Highlights are stored as flat span arrays in chunks of
lines, so the chunks before that line are shared with the previous update. Injected languages retain separate
trees, with a three-level injection limit. Non-local queries and retroactive syntax
changes can invalidate earlier content; stream updates are not a promise that
earlier lines will never change.

Empty updates do no parsing or highlighting. Cloned streams can append
independently. Unknown-language streams reuse preceding line projections without
parsing.

`MAX_SOURCE_BYTES` bounds each source.

`SyntaxStream::work_stats()` reports cumulative parser callback bytes, query range
bytes, full/incremental parse counts, projected bytes/lines, reused lines, and
structural nodes compared while refining error-wrapper invalidation.
`thread_work()` sums the same counters across every stream on the calling thread,
including injected languages, so hosts and tests measure renderer work as a
before/after difference.
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
counters. `SyntaxStream::append` reports input limits, missing grammars (including
bundled queries that fail to compile), language setup, and missing parse results;
`analyze` leaves rejected or unknown sources plain. On parser failure, source and
published highlights remain unchanged, and the next attempt rebuilds the discarded
parser state.

## Example

```rust
use clankerdiff_syntax::{SyntaxError, SyntaxStream, SyntaxTheme, analyze};

fn example() -> Result<(), SyntaxError> {
    let mut stream = SyntaxStream::new("rust");
    let update = stream.append("/* open\nclosed */")?;
    assert_eq!(update.highlights.line_count(), 2);
    assert_eq!(update.changed_lines, 0..2);

    let source = analyze("rust", "fn main() {}\n");
    assert_eq!(source.highlights.line_count(), 1);
    assert_eq!(source.symbols.definitions[0].name, "main");
    let theme = SyntaxTheme::default();
    let styled = source.highlights.line(0).unwrap_or_default().iter().filter_map(|span| theme.highlight(span.highlight));
    assert!(styled.count() > 0);
    Ok(())
}
```
