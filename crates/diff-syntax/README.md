# clankerdiff-syntax

Reusable Arborium syntax highlighting.

## Example

```rust
use clankerdiff_syntax::{SyntaxHighlighter, SyntaxStream, SyntaxStreamError};
use clankerdiff_theme::SyntaxTheme;

fn example() -> Result<(), SyntaxStreamError> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut stream = SyntaxStream::new("rust");
    let update = highlighter.with_theme(&theme)
        .append(&mut stream, ["/* open", "closed */"])?;
    assert_eq!(update.highlights.line_count(), 2);
    assert_eq!(update.changed_lines, 0..2);
    Ok(())
}
```
