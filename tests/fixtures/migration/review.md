# Migration acceptance document

A paragraph with **strong**, *emphasis*, `inline code`, and a [link](https://example.com/review).
Unicode: café, é, 日本語, and a tab:	between words.

## Lists and quotations

- First item
- Second item
  - Nested item with **formatting**

1. Ordered first
2. Ordered second

> A quoted paragraph with a link to [the outline](#tables).
>
> A second quoted paragraph.

## Tables

| Left | Center | Right |
| :--- | :----: | ----: |
| alpha | **beta** | 42 |
| 日本語 | `code` | 100 |

## Highlighted code

```rust
fn greeting(name: &str) -> String {
    format!("Hello, {name}! café 日本語")
}
```

```json
{"review": true, "comments": ["first", "second"]}
```

---

<div>HTML remains review content; it must not execute.</div>

## Reflow and comments

This deliberately long paragraph provides enough text to wrap when the viewport is resized or the interface scale changes. Its block identity and review anchor should remain attached to the same content, rather than to a mounted row index. Repeated theme and layout changes must not discard a draft or move the reviewer to an unrelated heading.
