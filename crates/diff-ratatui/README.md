# clankerdiff-ratatui

Ratatui adapters for syntax highlighting, Markdown parsing, diff previews, and review widgets.

## Streaming Markdown

Keep a `MarkdownStream`, `StreamingMarkdownState`, and `SyntaxHighlighter` per logical rendering context. Append only new source bytes, then call `MarkdownRenderer::render_stream_layout`. The returned `MarkdownLayout` shares row handles; `state.update_since(last_revision)` supplies the suffix to replace. Apply its `first_changed_row`, `replacement`, and `reset` fields together, then retain its `revision`. A missed revision returns a replacement sufficient to reconstruct the current live content.

The default `StreamingMarkdownPolicy::Reflowable` matches a fresh render of the current source. For native terminal scrollback, construct the state with `StreamingMarkdownPolicy::Terminal`:

1. Render and apply the row update.
2. Write the chosen leading rows to native history through the host's terminal writer.
3. Only after that write succeeds, call `state.commit_rows(revision, end_exclusive)` with an absolute layout row index. Do not count host-added padding or separators as Markdown rows.
4. On write failure, leave those rows uncommitted and retry through the host's writer.

Commits are monotonic and validate the layout revision and range. Committed rows retain their handles, source metadata, text, and styles. Resize and theme changes affect only the uncommitted suffix; checkpoints retain partial-row and tab-expansion progress. Later delimiters or reference definitions may intentionally make terminal history differ from a new full-document render. One-shot equality is not a promise that arbitrary Markdown or incomplete syntax has an eternally stable prefix.

Call `stream.finish()` and render its final update before sealing an item. Preserve the terminal state rather than replacing it with a one-shot render. `state.reset()` clears rendering caches, not acknowledged history. Replacing or switching a source after a commit returns `MarkdownStreamError::CommittedSourceReplaced`; create a separate item/state for replacement content.

Use `MarkdownRows` and row updates on the hot path. `materialize()` and `render_stream_lines()` are convenience boundaries for hosts that require flat arrays, not a requirement for streaming. `take_stats()` exposes parser, highlighter, and row work. `blocks_visited` counts semantic blocks visited during layout, `targets_visited` counts target-map entries inspected or updated, `chunks_visited` counts rendered/cached line chunks visited, and `row_store_updates` counts individual row insertions and persistent range splice/truncate operations. A range operation shares its prefix and costs logarithmic tree work; the counter does not claim to count the persistent collection's internal node copies. `rows_reused` describes logical output reuse, not rows traversed. `source_bytes_copied` counts explicit source retention and synthetic reference-prefix/tail copies (not allocator reallocations or semantic-node allocations); `prefix_bytes_copied` identifies the reference-prefix portion. These counters supplement actual parser input bytes rather than substituting appended bytes for parsing work.

Completed blocks and committed rows use persistent row storage. In `Rendered` presentation, ordinary appends visit changed blocks and code lines rather than rebuilding the completed prefix. `SourceLines` retains cached rows but may still traverse source-line prefixes; the rendered transcript's prefix-work bounds do not apply to that alternate presentation. Continuous structural tails, including long paragraphs, lists, quotes, and nested fences, may still require reparsing their entire unsettled block for CommonMark correctness. Their actual work is counted; the concrete prose/top-level-fence workload budgets are not a universal linear-time guarantee for arbitrary Markdown.

The upstream `just package-check` recipe packages and verifies the seven portable libraries together, then runs `tests/consumer-fixture` from a temporary directory against a temporary registry of those exact archives. It does not publish or read sibling application sources. Archive checksums, file lists, resolved versions, and the consumer lockfile are saved under `target/package-check`. Temporary registry patches are used only for this pre-publication check; `just published-consumer-check` runs the same fixture against the versions in the workspace manifests with crates.io dependencies only and no patches. Run that gate after the release workflow publishes the coordinated versions.

Run `cargo run -p clankerdiff-ratatui --example terminal_markdown` for a complete host-side write/acknowledgement example, including a failed write, revision validation, completion, and late reference resolution. `streaming_markdown` demonstrates reflowable rendering; `chat_embedding` and the review examples show library embedding without a second application event loop.
