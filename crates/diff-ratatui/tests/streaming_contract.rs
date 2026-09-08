use clankerdiff_markdown::MarkdownStream;
use clankerdiff_ratatui::testing::MarkdownStreamFixture;
use clankerdiff_theme::Rgba;
use std::sync::Arc;

#[test]
fn every_snapshot_matches_one_shot_at_each_utf8_boundary() {
    for source in [
        "[delayed][target]\n\n[target]: https://example.com\n",
        "- first\n\n- second\n  - nested **strong *emphasis***\n",
        "> ```rust\n> /* open\n> closed */\n> ```\n",
        "    indented\n\n    code\n",
        "# Héading\r\n\r\nText 世界 with `code` and \\*literal*.\r\n",
        "```rust\n/* open\nstill open",
        "| left | right |\n| :--- | ---: |\n| 界 | **bold** |\n",
        "- [ ] pending\n- [x] done\n\n```sh\necho \"$HOME\"\n```\n",
        "```typescript\nconst x: string = `multi\nline`;\n```\n",
    ] {
        let mut fixture = MarkdownStreamFixture::default();
        let mut previous = 0;
        for end in source
            .char_indices()
            .map(|(i, _)| i)
            .skip(1)
            .chain([source.len()])
        {
            fixture.stream.push(&source[previous..end]);
            fixture.assert_equivalent();
            previous = end;
        }
        fixture.stream.finish();
        fixture.assert_equivalent();
    }
}

#[test]
fn delayed_references_restyle_preceding_rows() {
    let mut fixture = MarkdownStreamFixture::default();
    fixture.stream.push("[delayed][target]\n\n");
    fixture.assert_equivalent();
    fixture.stream.push("[target]: https://example.com\n");
    fixture.assert_equivalent();
}

#[test]
fn equal_revisions_on_different_streams_do_not_reuse_rows() {
    let mut fixture = MarkdownStreamFixture::default();
    fixture.stream.push("first");
    fixture.assert_equivalent();
    let mut other = MarkdownStream::new();
    other.push("second");
    assert_eq!(fixture.stream.revision(), other.revision());
    fixture.stream = other;
    fixture.assert_equivalent();
}

#[test]
fn replacement_resize_palette_spacing_and_completion_match_one_shot() {
    let mut fixture = MarkdownStreamFixture::default();
    fixture
        .stream
        .push("# Heading\n\nA longer paragraph with **bold** text.\n\n```rust\n/* open\n");
    fixture.assert_equivalent();
    fixture.options.width = 7;
    fixture.assert_equivalent();
    fixture.options.block_spacing = false;
    fixture.assert_equivalent();
    fixture.theme.markdown.heading = Rgba::new(7, 11, 13, 255);
    fixture.assert_equivalent();
    fixture.stream.replace("short");
    fixture.assert_equivalent();
    fixture.stream.finish();
    fixture.assert_equivalent();
    fixture.stream.push(" again");
    fixture.assert_equivalent();
}

#[test]
fn long_multiline_fences_match_one_shot_after_each_requested_frame() {
    let mut fixture = MarkdownStreamFixture::default();
    fixture.stream.push("```rust\n/* opening comment\n");
    fixture.stream.push(&"still a comment\n".repeat(1_100));
    fixture.assert_equivalent();
    fixture.stream.push("let still_comment = 1;\n");
    fixture.assert_equivalent();
    fixture.stream.push("closed */\n```\n");
    fixture.assert_equivalent();
    fixture.stream.finish();
    fixture.assert_equivalent();
}

#[test]
fn resizing_and_palette_changes_reuse_the_semantic_parse_and_highlights() {
    let mut fixture = MarkdownStreamFixture::default();
    fixture
        .stream
        .push("# Heading\n\nText with **bold** words.\n\n```rust\nlet value = 1;\n```\n");
    fixture.render();
    fixture.state.take_stats();
    fixture.highlighter.take_stats();
    fixture.options.width = 5;
    fixture.assert_equivalent();
    assert_eq!(fixture.state.take_stats().parsed_bytes, 0);
    assert_eq!(fixture.highlighter.take_stats().bytes, 0);
    fixture.theme.markdown.heading = Rgba::new(1, 2, 3, 255);
    fixture.assert_equivalent();
    assert_eq!(fixture.state.take_stats().parsed_bytes, 0);
    assert_eq!(fixture.highlighter.take_stats().bytes, 0);
}

#[test]
fn appending_to_an_open_fence_reuses_closed_block_highlights() {
    let mut fixture = MarkdownStreamFixture::default();
    fixture
        .stream
        .push("```rust\nlet closed = 1;\n```\n\n```rust\n/* open\n");
    fixture.render();
    fixture.highlighter.take_stats();
    fixture.stream.push("still open\n");
    fixture.assert_equivalent();
    let appended = fixture.highlighter.take_stats();
    let mut open_only = MarkdownStreamFixture::default();
    open_only.stream.push("```rust\n/* open\nstill open\n");
    open_only.render();
    assert_eq!(appended.hits, 1);
    assert_eq!(appended.bytes, open_only.highlighter.take_stats().bytes);
}

#[test]
fn unchanged_redraw_reuses_storage_without_parsing_or_highlighting() {
    let mut fixture = MarkdownStreamFixture::default();
    fixture.stream.push("```rust\nlet value = 1;\n```\n");
    let first = fixture.render();
    let cold = fixture.state.take_stats();
    assert_eq!(cold.parsed_documents, 1);
    assert_eq!(cold.parsed_bytes, fixture.stream.source().len());
    assert_eq!(cold.rows_generated, first.len());
    fixture.highlighter.take_stats();
    let second = fixture.render();
    assert!(Arc::ptr_eq(&first, &second));
    let unchanged = fixture.state.take_stats();
    assert_eq!(unchanged.parsed_bytes, 0);
    assert_eq!(unchanged.rows_generated, 0);
    assert_eq!(unchanged.rows_reused, first.len());
    assert_eq!(fixture.highlighter.take_stats().bytes, 0);
}
