#![cfg(feature = "test-support")]

use clankerdiff_markdown::MarkdownStream;
use clankerdiff_ratatui::{
    MarkdownLayoutOptions, MarkdownPresentation, testing::MarkdownStreamFixture,
};
use ratatui::style::Modifier;
use std::{error::Error, sync::Arc};

#[test]
fn source_lines_preserve_identity_and_graphemes() -> Result<(), Box<dyn Error>> {
    let source = "# e\u{301}界\r\n\r\n```rust\r\nlet x = 1;\r\n```\r\n";
    let mut fixture =
        MarkdownStreamFixture::from_source(source).with_options(MarkdownLayoutOptions {
            presentation: MarkdownPresentation::SourceLines,
            wrap: false,
            ..MarkdownLayoutOptions::default()
        });
    let layout = fixture.layout();
    assert_eq!(layout.row_count(), 6);
    for index in 0..6 {
        assert_eq!(
            layout.rows_for_source_line(index + 1),
            Some(index..index + 1)
        );
        assert_eq!(
            layout
                .row(index)
                .ok_or("missing row")?
                .source
                .as_ref()
                .ok_or("missing mapping")?
                .lines
                .start,
            index + 1
        );
    }
    assert_eq!(
        layout.row(0).ok_or("missing row")?.line.to_string(),
        "# e\u{301}界"
    );
    fixture.options.width = 3;
    fixture.options.wrap = true;
    let wrapped = fixture.layout();
    assert!(wrapped.rows().iter().all(|row| row.line.width() <= 3));
    assert!(
        wrapped
            .rows_for_source_line(1)
            .ok_or("missing wrapped range")?
            .len()
            > 1
    );
    assert!(
        wrapped
            .rows()
            .iter()
            .any(|row| row.line.to_string().contains("e\u{301}"))
    );
    Ok(())
}

#[test]
fn chunk_slices_share_rows_and_materialization() -> Result<(), Box<dyn Error>> {
    let mut fixture =
        MarkdownStreamFixture::from_source("# One\n\nA paragraph\n\n## Two\n\nMore text");
    let layout = fixture.layout();
    for start in 0..=layout.row_count() {
        for end in start..=layout.row_count() {
            let slice = layout.rows().slice(start..end);
            assert_eq!(slice.len(), end - start);
            assert_eq!(slice.iter().count(), end - start);
            for (index, row) in slice.iter().enumerate() {
                assert!(Arc::ptr_eq(
                    row,
                    layout.row(start + index).ok_or("missing row")?
                ));
                assert!(Arc::ptr_eq(
                    row,
                    slice.get(index).ok_or("missing slice row")?
                ));
            }
            assert!(slice.get(slice.len()).is_none());
            assert!(
                slice.iter().rev().eq(layout
                    .rows()
                    .iter()
                    .skip(start)
                    .take(end - start)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev())
            );
        }
    }
    assert!(Arc::ptr_eq(
        &layout.materialize(),
        &layout.clone().materialize()
    ));
    let unchanged = fixture.layout();
    assert!(Arc::ptr_eq(
        layout.row(0).ok_or("missing row")?,
        unchanged.row(0).ok_or("missing row")?
    ));
    Ok(())
}

#[test]
fn streaming_updates_reconstruct_every_snapshot() {
    for source in [
        "# Hello\n\n**bold** and e\u{301}界\n",
        "[link][later]\n\n[later]: /target\n",
        "- one\n\n- two\n",
        "```rust\n/* open\n\nclose */\n```\n",
        "```rust\n\n\nlet value = 1;\n```\n",
    ] {
        let mut fixture = MarkdownStreamFixture::default();
        for character in source.chars() {
            fixture.stream.push(&character.to_string());
            fixture.assert_equivalent();
        }
        fixture.stream.finish();
        fixture.assert_equivalent();
        fixture.options.width = 7;
        fixture.assert_equivalent();
        fixture.options.presentation = MarkdownPresentation::SourceLines;
        fixture.assert_equivalent();
        fixture.stream = MarkdownStream::default();
        fixture.stream.push("short");
        fixture.assert_equivalent();
        fixture.state.reset();
        fixture.assert_equivalent();
    }
}

#[test]
fn stale_updates_reset_and_unchanged_updates_are_empty() {
    let mut fixture = MarkdownStreamFixture::from_source("first");
    fixture.apply_update();
    let unchanged = fixture.state.update_since(fixture.revision);
    assert!(!unchanged.reset);
    assert!(unchanged.replacement.is_empty());
    assert_eq!(unchanged.total_rows(), fixture.host.len());
    fixture.stream.push("\n\nsecond");
    fixture.apply_update();
    fixture.stream.push("\n\nthird");
    fixture.apply_update();
    let stale = fixture.state.update_since(0);
    assert!(stale.reset);
    assert_eq!(stale.first_changed_row, 0);
    assert!(stale.replacement.iter().eq(fixture.host.iter()));
}

#[test]
fn finishing_and_resuming_without_source_changes_do_not_repeat_work() {
    let mut fixture = MarkdownStreamFixture::from_source("```rust\nfn main() {}\n```");
    fixture.apply_update();
    fixture.state.take_stats();
    let revision = fixture.revision;
    fixture.stream.finish();
    let update = fixture.apply_update();
    assert_eq!(update.revision, revision);
    assert!(update.replacement.is_empty());
    let work = fixture.state.take_stats();
    assert_eq!(work.parsed_bytes, 0);
    assert_eq!(work.parsed_documents, 0);
    assert_eq!(work.highlighted_bytes, 0);
    assert_eq!(work.rows_generated, 0);
    fixture.stream.push("");
    fixture.apply_update();
    let work = fixture.state.take_stats();
    assert_eq!(work.parsed_documents, 0);
    assert_eq!(work.rows_generated, 0);
    fixture.stream.push("\n\nNew paragraph");
    fixture.assert_equivalent();
    assert!(fixture.state.take_stats().parsed_documents > 0);
}

#[test]
fn source_fitting_preserves_text_within_the_requested_width() {
    for (source, width, wrap, expected) in [
        ("", 2, true, vec![""]),
        ("a\n", 2, true, vec!["a", ""]),
        ("a\r\nb", 2, true, vec!["a", "b"]),
        ("e\u{301}👩‍💻界x", 2, true, vec!["e\u{301}", "👩‍💻", "界", "x"]),
        ("界x", 1, true, vec!["x"]),
        ("界x", 1, false, vec![""]),
        ("abc\ndef", 0, true, vec!["", ""]),
        ("abcdef\nxyz", 2, false, vec!["ab", "xy"]),
        ("a\tb", 2, true, vec!["a ", "  ", "b"]),
    ] {
        let mut fixture =
            MarkdownStreamFixture::from_source(source).with_options(MarkdownLayoutOptions {
                width,
                wrap,
                presentation: MarkdownPresentation::SourceLines,
                tab_width: 4,
                ..Default::default()
            });
        let actual: Vec<_> = fixture
            .layout()
            .rows()
            .iter()
            .map(|row| row.line.to_string())
            .collect();
        assert_eq!(
            actual, expected,
            "source {source:?}, width {width}, wrap {wrap}"
        );
    }
}

#[test]
fn graphemes_crossing_style_boundaries_remain_intact() -> Result<(), Box<dyn Error>> {
    let mut fixture = MarkdownStreamFixture::from_source("**e**\u{301}xy");
    fixture.options.width = 1;
    let layout = fixture.layout();
    assert_eq!(
        layout
            .row(0)
            .ok_or("missing combined row")?
            .line
            .to_string(),
        "e\u{301}"
    );
    assert!(
        layout.row(0).ok_or("missing combined row")?.line.spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    Ok(())
}

#[test]
fn continuation_prefixes_leave_room_for_source_text() {
    for (source, width, expected) in [
        ("> abcdef", 4, vec!["│ ab", "│ cd", "│ ef"]),
        ("- abcdef", 4, vec!["• ab", "  cd", "  ef"]),
        ("> ab", 1, vec!["│", " ", "a", "b"]),
        ("> ab", 2, vec!["│ ", "│a", "│b"]),
        ("> a  \n> b", 4, vec!["│ a", "b"]),
    ] {
        let mut fixture = MarkdownStreamFixture::from_source(source);
        fixture.options.width = width;
        let rows: Vec<_> = fixture
            .layout()
            .rows()
            .iter()
            .map(|row| row.line.to_string())
            .collect();
        assert_eq!(rows, expected, "{source}");
    }
}
