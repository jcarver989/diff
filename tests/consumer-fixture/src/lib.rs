#[cfg(test)]
mod tests {
    #[cfg(feature = "git")]
    use clankerdiff_git::GitRepository;
    #[cfg(feature = "git")]
    use clankerdiff_ratatui::diff::DiffScope;
    use clankerdiff_ratatui::{
        DiffPreviewOptions, DiffPreviewState, DiffReviewState, DiffReviewWidget,
        MarkdownLayoutOptions, MarkdownRenderer, MarkdownReviewState, MarkdownReviewWidget,
        StreamingMarkdownPolicy, StreamingMarkdownState,
        diff::{DiffDocument, FileDiff, ViewMode},
        markdown::{
            MarkdownDocument, MarkdownReviewCommand, MarkdownReviewDecision, MarkdownReviewEvent,
            MarkdownStream,
        },
        syntax::{LanguageHint, SyntaxHighlighter, resolve_language},
        theme::{ReviewTheme, ThemeChoice, ThemeId},
    };
    use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
    use std::{error::Error, str, sync::Arc};
    #[cfg(feature = "git")]
    use std::{fs, process::Command};
    #[cfg(feature = "git")]
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
    fn preview_and_review_events_use_facade_types() -> TestResult {
        let theme = ReviewTheme::builtin("sage")?;
        let choice = ThemeChoice::new("Consumer", theme.clone());
        let mut highlighter = SyntaxHighlighter::default();
        assert_eq!(
            resolve_language(LanguageHint::Path("file.rs"), ""),
            Some("rust")
        );
        let mut preview = DiffPreviewState::new(FileDiff::from_texts("file.rs", "old\n", "new\n")?);
        let lines = preview.render(
            80,
            &theme,
            &mut highlighter,
            DiffPreviewOptions {
                view_mode: ViewMode::Unified,
                ..DiffPreviewOptions::default()
            },
        );
        assert!(!lines.is_empty());
        let mut review =
            MarkdownReviewState::new(Arc::new(MarkdownDocument::parse("# Plan\n\nShip it.\n")));
        review.set_theme_choices(vec![choice]);
        let event = review
            .handle_command(MarkdownReviewCommand::Approve)?
            .into_event();
        let Some(MarkdownReviewEvent::Submit(submission)) = event else {
            return Err("expected a review submission".into());
        };
        assert_eq!(submission.decision, MarkdownReviewDecision::Approved);
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

    #[cfg(feature = "git")]
    #[tokio::test]
    async fn git_snapshots_supply_real_repository_sources() -> TestResult {
        let directory = RepositoryBuilder.build()?;
        let repository = GitRepository::discover(directory.path()).await?;
        let snapshot = repository.snapshot_with_sources(DiffScope::Both).await?;
        assert!(!snapshot.document.files.is_empty());
        let mut state = DiffReviewState::new(snapshot.document);
        let area = Rect::new(0, 0, 80, 24);
        DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut state);
        Ok(())
    }

    #[cfg(feature = "watch")]
    #[tokio::test]
    async fn repository_watcher_supplies_renderable_snapshots_and_accepts_requests() -> TestResult {
        use clankerdiff_watch::{RepositoryRequest, RepositoryWatcher, WatchOptions};
        use std::time::Duration;
        use tokio::{sync::oneshot, time::timeout};

        let directory = RepositoryBuilder.build()?;
        let repository = GitRepository::discover(directory.path()).await?;
        let mut watcher =
            RepositoryWatcher::spawn(repository, DiffScope::Both, WatchOptions::default()).await?;
        let initial = watcher.state_rx.borrow_and_update().snapshot.clone();
        let mut state = DiffReviewState::new(Arc::clone(&initial.document));
        let area = Rect::new(0, 0, 80, 24);
        DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut state);

        fs::write(directory.path().join("file.rs"), "let changed_again = 3;\n")?;
        timeout(Duration::from_secs(10), async {
            loop {
                watcher.state_rx.changed().await?;
                let retained = watcher.state_rx.borrow_and_update().clone();
                if let Some(error) = retained.error_message() {
                    break Err(error.into());
                }
                if retained.snapshot.document != initial.document {
                    break Ok::<(), Box<dyn Error>>(());
                }
            }
        })
        .await??;

        let (result_tx, result_rx) = oneshot::channel();
        watcher
            .request_tx
            .send(RepositoryRequest::SetScope {
                scope: DiffScope::Staged,
                result_tx,
            })
            .await?;
        timeout(Duration::from_secs(10), result_rx)
            .await??
            .map_err(|error| error.to_string())?;
        let staged = watcher.state_rx.borrow().snapshot.clone();
        assert_eq!(staged.scope, DiffScope::Staged);
        assert!(staged.document.files.is_empty());
        Ok(())
    }

    #[cfg(feature = "git")]
    #[derive(Default)]
    struct RepositoryBuilder;

    #[cfg(feature = "git")]
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

    #[cfg(feature = "git")]
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
