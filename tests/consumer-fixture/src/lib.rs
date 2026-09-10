#[cfg(test)]
mod tests {
    use clankerdiff_core::{DiffDocument, DiffScope, FileDiff};
    use clankerdiff_git::GitRepository;
    use clankerdiff_markdown::{MarkdownDocument, MarkdownStream};
    use clankerdiff_ratatui::{
        DiffReviewState, DiffReviewWidget, MarkdownLayoutOptions, MarkdownRenderer,
        MarkdownReviewState, MarkdownReviewWidget, StreamingMarkdownPolicy, StreamingMarkdownState,
    };
    use clankerdiff_syntax::SyntaxHighlighter;
    use clankerdiff_theme::{ReviewTheme, ThemeId};
    use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
    use std::{error::Error, fs, process::Command, str, sync::Arc};
    use tempfile::{TempDir, tempdir};

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn themes_and_review_widgets_embed_without_a_terminal_loop() -> TestResult {
        let theme = ReviewTheme::builtin("sage")?;
        let restored =
            ReviewTheme::from_bytes(ThemeId::Custom("consumer".into()), &theme.to_bytes()?)?;
        let file = FileDiff::from_texts("src/lib.rs", "let old = 1;\n", "let new = 2;\n")?;
        let mut diff = DiffReviewState::new(Arc::new(DiffDocument {
            repo_root: String::new(),
            files: vec![file],
        }));
        diff.set_theme(restored.clone());
        let mut markdown = MarkdownReviewState::new(Arc::new(MarkdownDocument::parse(
            "# Plan\n\nReview **this**.\n\n```rust\nfn main() {}\n```\n",
        )));
        markdown.set_theme(restored);
        let area = Rect::new(3, 2, 80, 24);
        let mut buffer = Buffer::empty(area);
        DiffReviewWidget::new()
            .title("Diff")
            .render(area, &mut buffer, &mut diff);
        assert!(buffer.content().iter().any(|cell| cell.symbol() != " "));
        buffer.reset();
        MarkdownReviewWidget::new()
            .title("Plan")
            .render(area, &mut buffer, &mut markdown);
        assert!(buffer.content().iter().any(|cell| cell.symbol() != " "));
        let _ = (diff.cursor_position(), markdown.cursor_position());
        Ok(())
    }

    #[test]
    fn published_stream_contract_preserves_history_and_work_budgets() -> TestResult {
        for (source, code) in [
            (
                "Ordinary words in a bounded paragraph.\n\n".repeat(440),
                false,
            ),
            (
                format!("```rust\n{}\n```\n", "let value = 123;\n".repeat(1600)),
                true,
            ),
        ] {
            let renderer = MarkdownRenderer::new();
            let mut highlighter = SyntaxHighlighter::default();
            let mut stream = MarkdownStream::new();
            let mut state = StreamingMarkdownState::new(StreamingMarkdownPolicy::Terminal);
            let theme = ReviewTheme::default();
            let options = MarkdownLayoutOptions {
                width: 120,
                ..MarkdownLayoutOptions::default()
            };
            let mut host = Vec::new();
            let mut revision = 0;
            for chunk in source.as_bytes().chunks(256) {
                stream.push(str::from_utf8(chunk)?);
                renderer.render_stream_layout(
                    &mut state,
                    &stream,
                    options,
                    &theme,
                    &mut highlighter,
                )?;
                let delta = state.update_since(revision);
                assert!(delta.first_changed_row >= state.committed_rows());
                host.truncate(delta.first_changed_row);
                host.extend(delta.replacement.iter().cloned());
                revision = delta.revision;
                if host.len() - state.committed_rows() > 12 {
                    state.commit_rows(revision, host.len() - 1)?;
                    assert!(
                        state.update_since(delta.base_revision).first_changed_row
                            >= state.committed_rows()
                    );
                }
                assert!(host.len() - state.committed_rows() <= 24);
            }
            stream.finish();
            let layout = renderer.render_stream_layout(
                &mut state,
                &stream,
                options,
                &theme,
                &mut highlighter,
            )?;
            let expected = renderer.render_layout(
                &MarkdownDocument::parse(&source),
                options,
                &theme,
                &mut SyntaxHighlighter::default(),
            );
            assert_eq!(layout.row_count(), expected.row_count());
            assert!(
                layout
                    .rows()
                    .iter()
                    .zip(expected.rows().iter())
                    .all(|(a, b)| a.line == b.line)
            );
            let work = state.take_stats();
            let budget = 4 * source.len() + 64 * 1024;
            assert!(work.parsed_bytes <= budget, "{work:?}");
            assert!(!code || work.highlighted_bytes <= budget, "{work:?}");
            assert!(work.source_bytes_copied <= budget, "{work:?}");
            assert!(work.blocks_visited <= budget, "{work:?}");
            assert!(work.targets_visited <= budget, "{work:?}");
            assert!(work.row_store_updates <= budget, "{work:?}");
            renderer.render_stream_layout(
                &mut state,
                &stream,
                options,
                &theme,
                &mut highlighter,
            )?;
            let settled = state.take_stats();
            assert_eq!(
                settled.parsed_bytes
                    + settled.highlighted_bytes
                    + settled.rows_generated
                    + settled.blocks_visited
                    + settled.targets_visited
                    + settled.row_store_updates,
                0
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn git_snapshots_supply_real_repository_sources() -> TestResult {
        let directory = RepositoryBuilder::default().build()?;
        let repository = GitRepository::discover(directory.path()).await?;
        let snapshot = repository.snapshot_with_sources(DiffScope::Both).await?;
        assert!(!snapshot.document.files.is_empty());
        let mut state = DiffReviewState::new(snapshot.document);
        let area = Rect::new(0, 0, 80, 24);
        DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut state);
        Ok(())
    }

    #[derive(Default)]
    struct RepositoryBuilder;

    impl RepositoryBuilder {
        fn build(self) -> Result<TempDir, Box<dyn Error>> {
            let directory = tempdir()?;
            for arguments in [
                vec!["init", "--quiet"],
                vec!["config", "user.name", "Consumer"],
                vec!["config", "user.email", "consumer@example.invalid"],
            ] {
                run_git(&directory, &arguments)?;
            }
            fs::write(directory.path().join("file.rs"), "let old = 1;\n")?;
            run_git(&directory, &["add", "file.rs"])?;
            run_git(
                &directory,
                &[
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "--quiet",
                    "-m",
                    "initial",
                ],
            )?;
            fs::write(directory.path().join("file.rs"), "let new = 2;\n")?;
            Ok(directory)
        }
    }

    fn run_git(directory: &TempDir, arguments: &[&str]) -> TestResult {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(directory.path())
            .output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        Ok(())
    }
}
