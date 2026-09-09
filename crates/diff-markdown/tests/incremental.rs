use clankerdiff_markdown::{MarkdownBlockKind, MarkdownDocument, MarkdownStream};
use std::{error::Error, fmt::Write};

type TestResult = Result<(), Box<dyn Error>>;

const CORPUS: &[&str] = &[
    "",
    "plain",
    "# Hello\n\n**bold** and e\u{301}界\n",
    "para one\npara one continued\n\npara two\n",
    "Streaming contract\n===\n\nsetext above\n",
    "text\n---\n\n***\n\nafter rule\n",
    "# h\npara directly after\n# h2\n",
    "- one\n- two\n\n- three\n\n\n  continuation\n\n1. a\n2. b\n\n1\n",
    "1. outer\n   - inner\n   - second\n2. last\n\n* other\n+ plus\n",
    "- [ ] task\n- [x] done\n",
    "    indented\n\n    still code\n\nafter\n\n    again\n",
    "```rust\nfn main() {\n    let s = \"multi\nline\";\n}\n```\n\ndone\n",
    "```\ncode\n```",
    "```\ncode\n```\n",
    "```\ncode\n  ``` \nafter\n",
    "```\ncode\n    ```\nnot closed\n```\n\nx\n",
    "```\n```x\n```\n",
    "```\n\n```\n\nx\n",
    "```\n\r\n```\r\n\r\nx\r\n",
    "````\n```\nstill code\n````\n\nout\n",
    "~~~text\nthis ~~ fence holds ``` backtick ``` lines inside\n~~~\n\n```js\nlet first = 1;\n```\n```text\nsecond\n```\n",
    "```a`b\nnot a fence\n```\n",
    "   ```rust\n   indented fence\n   ```\n\ntext\n",
    "\t```\ntab fence is code\n",
    "para\n```rust\ninterrupts\n```\nback\n",
    "```rust\nfn unclosed(",
    "```rust\nfn main() {}\n```\rfoo\n",
    "```rust\ncode\n```\r",
    "```rust\ncode\n```  ",
    "```rust\ncode\n```\t\nmore\n```\n",
    "> quote\n> more\n\n> second\n\nlazy\n> q\ncontinued\n",
    "> # quoted\n>\n> ```rust\n> code\n> ```\n> text\n",
    "- item\n\n  ```rust\n  nested\n  ```\n\n- next\n",
    "| a | b |\n| --- | :-: |\n| 1 | 2 |\n\nx\n",
    "| a |\n|---|\n| 1 |\nno pipe\n\n| c |\n",
    "<div>\nhtml\n\nafter\n",
    "<pre>\nx\n\ny\n</pre>\n\nz\n",
    "<!-- comment\n\nstill -->\ntext\n",
    "[link][later]\n\n[later]: /target\n",
    "[a]: /u 'ti'\n\n[a] and [A]\n",
    "[a]: /u\n[a]: /v\n\n[A]\n\n[a]: /w\n",
    "> [b]: /q\n\n[b]\n",
    "[c]:\n/u\n\n[c]\n",
    "[d]: /u 'unterminated\n\n[d]\n",
    "[e] early\n\ntext\n\n[e]: /late\n\n![img][e]\n",
    "use [x] here\n\n```\n[x]: /not-a-def\n```\n\n[x]: /real\n",
    "text with [brackets] but no defs\n\nmore\n",
    "[link](url) ![alt](image) `code` ~~gone~~ _em_ **strong**\n",
    "a\tb\n\n  leading spaces\n\ntrailing spaces  \nhard break\\\nnext\n",
    "\n\n\n",
    "   \n\t\n",
    "\u{a0}nbsp is not blank\n\u{a0}\nstill paragraph\n",
    "x\n\x0c\nform feed blank\n",
    "line\r\nwindows\r\n\r\n```py\r\nprint('x')\r\n```\r\n\r\nend\r\n",
    "# unclosed **emphasis\n\n```rust\ncode",
    "1) paren\n2) list\n\n2. dot\n",
    "- a\n\n\n- b\n",
    "-\n- empty item\n",
    "* * *\n- - -\n",
    "Foo\n===\nBar\n",
    "Foo\n= = =\n",
    "term\n: not definition list\n",
    "&amp; entities &copy; and \\*escapes\\*\n",
    "auto <https://example.com> link\n",
    "***bold italic*** and ___both___\n",
    "line one  \nline two\n",
    "> - quoted list\n> - two\n>\n> para\n",
    "1. a\n\n   b\n\n   ```\n   c\n   ```\n2. d\n",
    "```\n```\n```\n```\n",
];

#[test]
fn every_prefix_of_every_source_matches_the_one_shot_parse() -> TestResult {
    for source in CORPUS {
        for chunk_chars in [1, 3, 8, 64] {
            let mut stream = MarkdownStream::new();
            let mut characters = source.chars();
            loop {
                let chunk: String = characters.by_ref().take(chunk_chars).collect();
                if chunk.is_empty() {
                    break;
                }
                stream.push(&chunk);
                assert_matches_one_shot(&stream)?;
            }
            stream.finish();
            assert_matches_one_shot(&stream)?;
            assert_eq!(stream.source(), *source);
        }
    }
    Ok(())
}

#[test]
fn concatenated_corpus_documents_stream_correctly() -> TestResult {
    let mut stream = MarkdownStream::new();
    for source in CORPUS {
        for chunk in source.as_bytes().chunks(5) {
            stream.push(std::str::from_utf8(chunk)?);
            assert_matches_one_shot(&stream)?;
        }
        stream.push("\n\n");
        assert_matches_one_shot(&stream)?;
    }
    assert!(stream.settled_blocks() > 50, "{}", stream.settled_blocks());
    Ok(())
}

#[test]
fn aether_exactness_message_matches_at_every_chunk() -> TestResult {
    let message = exactness_message();
    for chunk_bytes in [7, 37, 128, 1024] {
        let mut stream = MarkdownStream::new();
        for chunk in chunk_message(&message, chunk_bytes) {
            stream.push(&chunk);
            assert_matches_one_shot(&stream)?;
        }
        stream.finish();
        assert_matches_one_shot(&stream)?;
    }
    Ok(())
}

#[test]
fn generated_documents_match_the_one_shot_parse() -> TestResult {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..3000 {
        let source = generate(&mut seed);
        let mut stream = MarkdownStream::new();
        for chunk in chunk_message(&source, 1 + (seed % 5) as usize) {
            stream.push(&chunk);
            assert_matches_one_shot(&stream)?;
        }
    }
    Ok(())
}

#[test]
fn streaming_prose_has_bounded_parser_work() -> TestResult {
    let source = prose_message(16 * 1024);
    let mut stream = MarkdownStream::new();
    for chunk in chunk_message(&source, 256) {
        stream.push(&chunk);
    }
    stream.finish();
    let stats = stream.parse_stats();
    let budget = 4 * source.len() + 64 * 1024;
    assert!(stats.parsed_bytes <= budget, "{stats:?}, budget {budget}");
    assert!(stats.scanned_bytes <= budget, "{stats:?}, budget {budget}");
    assert_eq!(stats.unsettled_blocks, 0);
    assert!(stream.settled_blocks() + 1 >= stream.document().blocks().len());
    assert_matches_one_shot(&stream)
}

#[test]
fn streaming_code_fence_has_bounded_parser_work() -> TestResult {
    let source = code_block_message(24 * 1024);
    let mut stream = MarkdownStream::new();
    let mut parses_while_open = 0;
    for chunk in chunk_message(&source, 256) {
        let before = stream.parse_stats().parses;
        stream.push(&chunk);
        if stream.open_code_block().is_some() && before > 0 {
            parses_while_open += stream.parse_stats().parses - before;
        }
    }
    stream.finish();
    let stats = stream.parse_stats();
    let budget = 4 * source.len() + 64 * 1024;
    assert!(stats.parsed_bytes <= budget, "{stats:?}, budget {budget}");
    assert!(stats.scanned_bytes <= budget, "{stats:?}, budget {budget}");
    assert_eq!(parses_while_open, 0);
    assert!(stats.parses <= 4, "{stats:?}");
    assert!(stream.open_code_block().is_none());
    assert_eq!(stream.settled_blocks(), 1);
    assert_matches_one_shot(&stream)
}

#[test]
fn tiny_chunks_scale_linearly_with_input_size() -> TestResult {
    let mut previous = None;
    for size in [8 * 1024, 16 * 1024, 32 * 1024] {
        let mut work = 0;
        for source in [prose_message(size), code_block_message(size)] {
            let mut stream = MarkdownStream::new();
            for chunk in chunk_message(&source, 3) {
                stream.push(&chunk);
            }
            let stats = stream.parse_stats();
            work += stats.parsed_bytes + stats.scanned_bytes;
            assert_matches_one_shot(&stream)?;
        }
        if let Some(previous) = previous {
            assert!(
                work <= previous * 5 / 2,
                "doubling input {size} grew work from {previous} to {work}"
            );
        }
        previous = Some(work);
    }
    Ok(())
}

#[test]
fn open_fences_report_their_block_and_close_precisely() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("intro\n\n```rust\n");
    assert_eq!(stream.open_code_block(), Some(1));
    assert_eq!(stream.settled_blocks(), 1);
    let parses = stream.parse_stats().parses;
    stream.push("let a = 1;\n``");
    assert_eq!(stream.open_code_block(), Some(1));
    assert_eq!(stream.parse_stats().parses, parses);
    stream.push("`");
    assert_matches_one_shot(&stream)?;
    assert_eq!(stream.open_code_block(), Some(1));
    stream.push("x\n");
    assert_matches_one_shot(&stream)?;
    assert_eq!(stream.open_code_block(), Some(1));
    stream.push("```\nafter\n");
    assert_matches_one_shot(&stream)?;
    assert_eq!(stream.open_code_block(), None);
    assert_eq!(stream.settled_blocks(), 2);
    let MarkdownBlockKind::CodeBlock(code) = &stream.document().blocks()[1].kind else {
        return Err("expected code block".into());
    };
    assert_eq!(
        code.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        ["let a = 1;", "```x"]
    );
    Ok(())
}

#[test]
fn late_reference_definitions_reparse_only_bracketed_blocks() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("plain paragraph\n\nsee [docs] now\n\nanother\n\n");
    assert_eq!(stream.settled_blocks(), 3);
    let revision = stream.revision();
    let update = stream.push("[docs]: https://example.com\n");
    assert_eq!(update.changed_blocks.start, 1);
    assert_eq!(stream.changes_since(revision).first_block, 1);
    assert_eq!(stream.parse_stats().unsettled_blocks, 1);
    assert_matches_one_shot(&stream)?;
    let MarkdownBlockKind::Paragraph { content } = &stream.document().blocks()[1].kind else {
        return Err("expected paragraph".into());
    };
    assert!(format!("{content:?}").contains("Link"));
    Ok(())
}

#[test]
fn change_tracking_reports_the_earliest_block_since_a_revision() -> TestResult {
    let mut stream = MarkdownStream::new();
    let first = stream.push("one\n\n");
    assert_eq!(first.changed_blocks, 0..1);
    let second = stream.push("two\n\n");
    assert_eq!(second.changed_blocks, 1..2);
    let third = stream.push("three");
    assert_eq!(third.changed_blocks, 2..3);
    assert_eq!(stream.changes_since(first.revision).first_block, 1);
    assert_eq!(stream.changes_since(second.revision).first_block, 2);
    assert_eq!(stream.changes_since(third.revision).first_block, 3);
    assert!(!stream.changes_since(0).replaced);
    assert_eq!(stream.changes_since(0).first_block, 0);
    let finished = stream.finish();
    assert_eq!(finished.changed_blocks, 3..3);
    assert_eq!(stream.changes_since(third.revision).first_block, 3);
    let replaced = stream.replace("new");
    assert_eq!(replaced.changed_blocks, 0..1);
    assert!(stream.changes_since(finished.revision).replaced);
    assert!(!stream.changes_since(replaced.revision).replaced);
    assert_eq!(stream.changes_since(replaced.revision).first_block, 1);
    assert_matches_one_shot(&stream)?;
    stream.push("\n\n```\nopen");
    assert_matches_one_shot(&stream)?;
    assert_eq!(stream.open_code_block(), Some(1));
    Ok(())
}

#[test]
fn cloned_streams_parse_independently() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("```rust\nshared\n");
    let mut cloned = stream.clone();
    cloned.push("```\n\nclosed\n");
    stream.push("still open\n");
    assert_matches_one_shot(&stream)?;
    assert_matches_one_shot(&cloned)?;
    assert_eq!(stream.open_code_block(), Some(0));
    assert_eq!(cloned.open_code_block(), None);
    Ok(())
}

#[test]
fn growing_structural_tails_do_not_reprocess_settled_prefixes() -> TestResult {
    for tail in [
        "ordinary words ".repeat(128),
        "- item with **emphasis**\n".repeat(96),
        "> quoted words\n".repeat(96),
        format!("- parent\n\n  ```rust\n{}", "  let value = 1;\n".repeat(96)),
    ] {
        let mut stream = MarkdownStream::new();
        stream.push(&"# settled heading\n\n".repeat(256));
        let before = stream.parse_stats();
        let prefix_blocks = stream.document().blocks().len();
        let mut tail_bytes = 0;
        let mut expected_parse_work = 0;
        for chunk in chunk_message(&tail, 17) {
            tail_bytes += chunk.len();
            expected_parse_work += tail_bytes + 2;
            stream.push(&chunk);
            assert_matches_one_shot(&stream)?;
        }
        let after = stream.parse_stats();
        assert_eq!(stream.settled_blocks(), prefix_blocks);
        assert!(after.parsed_bytes - before.parsed_bytes <= expected_parse_work);
        assert_eq!(
            after.source_bytes_copied - before.source_bytes_copied,
            tail.len()
        );
        assert_eq!(after.prefix_bytes_copied, before.prefix_bytes_copied);
        assert!(after.parsed_bytes - before.parsed_bytes >= tail.len());
    }
    Ok(())
}

#[test]
fn reference_prefix_copying_is_reported_separately() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("[docs]: /destination\n\n# settled\n\n");
    let before = stream.parse_stats();
    stream.push("use [docs]");
    let after = stream.parse_stats();
    assert!(after.prefix_bytes_copied > before.prefix_bytes_copied);
    assert!(after.source_bytes_copied - before.source_bytes_copied > "use [docs]".len());
    assert_matches_one_shot(&stream)
}

fn assert_matches_one_shot(stream: &MarkdownStream) -> TestResult {
    let expected = MarkdownDocument::parse(stream.source());
    let actual = stream.document();
    if actual != &expected {
        let blocks = expected.blocks().len();
        for (index, (left, right)) in actual
            .blocks()
            .iter()
            .zip(expected.blocks().iter())
            .enumerate()
        {
            if left != right {
                return Err(format!(
                    "block {index} of {blocks} differs for source {:?}\n incremental: {left:#?}\n one-shot: {right:#?}",
                    stream.source()
                )
                .into());
            }
        }
        return Err(format!(
            "document differs for source {:?}\n incremental: blocks {}, targets {:?}, outline {:?}, styles {:?}\n one-shot: blocks {}, targets {:?}, outline {:?}, styles {:?}",
            stream.source(),
            actual.blocks().len(),
            actual.targets(),
            actual.outline(),
            actual.source_styles(),
            expected.blocks().len(),
            expected.targets(),
            expected.outline(),
            expected.source_styles(),
        )
        .into());
    }
    Ok(())
}

fn exactness_message() -> String {
    let mut message = String::from("Streaming contract\n===\n\n");
    message.push_str(
        "The paragraph above is a setext heading, so no line of it may be finalized early.\n\n",
    );
    message.push_str("```rust\nfn one() {\n    let s = \"starts here\n");
    for index in 0..40 {
        let _ = writeln!(message, "and continues {index}");
    }
    message.push_str("    ```\n");
    message.push_str("ends here\";\n\nlet two = one();\n}\n```\n\n");
    message.push_str("Between blocks.\n\n```python\ndef f():\n    doc = \"\"\"First line\n");
    for index in 0..40 {
        let _ = writeln!(message, "doc line {index}");
    }
    message.push_str("last line\"\"\"\n    return doc\n```\n\n");
    message.push_str("~~~text\nthis ~~ fence holds ``` backtick ``` lines inside\n~~~\n\n");
    message.push_str("```js\nlet first = 1;\n```\n```text\nsecond\n```\n\n");
    message.push_str("Tail paragraph before the still-open block.\n\n```rust\nfn unclosed(");
    message.push_str(&"x".repeat(600));
    message
}

fn prose_message(total_bytes: usize) -> String {
    let mut message = String::new();
    let mut sentence = 0;
    while message.len() < total_bytes {
        for _ in 0..4 {
            let _ = write!(
                message,
                "Sentence {sentence} carries ordinary words so wrapping and parsing do real work. "
            );
            sentence += 1;
        }
        message.push_str("\n\n");
    }
    message
}

fn code_block_message(total_bytes: usize) -> String {
    let mut message = String::from("```rust\n");
    let mut line = 0;
    while message.len() < total_bytes {
        let _ = writeln!(
            message,
            "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");"
        );
        line += 1;
    }
    message.push_str("```\n");
    message
}

fn chunk_message(message: &str, chunk_bytes: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut rest = message;
    while !rest.is_empty() {
        let mut end = rest.len().min(chunk_bytes);
        while !rest.is_char_boundary(end) {
            end += 1;
        }
        chunks.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    chunks
}

fn generate(seed: &mut u64) -> String {
    const PIECES: &[&str] = &[
        "para ",
        "[x]",
        "[x]: /u\n",
        "[y]: /v 't'\n",
        "\n",
        "\n\n",
        "# h\n",
        "===\n",
        "---\n",
        "- item\n",
        "  cont\n",
        "1. one\n",
        "> q\n",
        "```\n",
        "```rust\n",
        "~~~\n",
        "    code\n",
        "| a | b |\n",
        "|---|---|\n",
        "<div>\n",
        "</div>\n",
        "<pre>\n",
        "`tick`",
        "**b**",
        "\r\n",
        "\t",
        "  ",
        "  ```\n",
        "    ```\n",
        "``` \n",
        "e\u{301}界",
        "![i][x]",
    ];
    let mut next = || {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    };
    let count = 1 + (next() % 24) as usize;
    (0..count)
        .map(|_| {
            PIECES[usize::try_from(next() % PIECES.len() as u64)
                .expect("index is below PIECES.len()")]
        })
        .collect()
}
