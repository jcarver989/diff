# Diff

Diff is a renderer-independent diff and review model with Ratatui and GPUI frontends. `clankerdiff` provides interactive Git and Markdown review applications.

## Workspace architecture

The current workspace separates the diff domain, Git operations, UI adapters, applications, and public subprocess protocol:

```text
diff-fingerprint          content fingerprints shared across the workspace
diff-theme                renderer-neutral colors, palettes, and syntax themes
diff-syntax               portable Arborium highlighting and bounded sequences
diff-markdown             semantic Markdown, append streams, and optional review state
diff-core                 diff documents, parsing, presentation, and review state
diff-git                  Git snapshots and repository mutations
diff-ratatui              feature-gated terminal rendering and review adapters
diff-gpui                 GPUI review components
diff-gpui-{desktop,web}   application shells
clankerdiff-protocol      versioned machine-readable review results
clankerdiff               CLI orchestration
```

New embedding code should use the portable crates directly. `diff-core` now exposes only the renderer-neutral diff domain; themes, highlighting, and Markdown APIs live in their dedicated portable crates.

### Ratatui feature selection

`diff-ratatui` has no review UI in its smallest configurations:

| Feature | Surface |
|---|---|
| `syntax` | neutral highlight spans to Ratatui `Line` conversion |
| `diff-preview` | bounded, replaceable `FileDiff` previews; implies `syntax` |
| `markdown` | whole-document and streaming read-only Markdown; implies `syntax` |
| `diff-review` | interactive diff review and Crossterm input |
| `markdown-review` | interactive Markdown review and Crossterm input |

A transcript host typically selects `default-features = false` with `features = ["syntax", "diff-preview", "markdown"]`. Auto diff layout uses split view at 96 columns, and previews default to 20 content rows plus an overflow summary. `MarkdownStream::push`, `replace`, and `finish` distinguish append updates, source resets, and final-tail stabilization. Stable prose is committed once; open-fence tails are rendered from bounded stable syntax context without committing speculative state. Width, theme, replacement, or backwards-offset changes invalidate the cache.

`diff-syntax` keeps the small `common-languages` bundle as its default. Embedders rendering coding-agent output can select `default-features = false, features = ["agent-languages"]` for a broad curated grammar set. It intentionally forwards granular permissively licensed Arborium features rather than `all-languages`, excluding GPL `lang-nginx`. Syntax sequence recovery retains at most `CacheConfig::context_lines`; constructs opened before that window use Tree-sitter recovery rather than exact lexer-state continuation. `SourceSequenceId` is an ordered-line, snapshot-local cache identity and must not be persisted.

Versioned Diff JSON is the only theme format; there is no runtime TextMate theme import, and the dependency graph contains no Syntect. See [the Wisp integration boundary](docs/wisp-integration.md) and the `streaming_markdown` example for dependency selection, ownership, instrumentation, and migration order.

## Clankerdiff subprocess protocol

Query capabilities before relying on a protocol or UI:

```sh
clankerdiff capabilities --format=json
```

Protocol version 1 uses a tagged `document_kind` response. Consumers should deserialize with `clankerdiff-protocol` and call `parse_response`, which rejects unsupported versions and invalid outcome/submission combinations while accepting unknown fields.

For JSON review commands:

- stdout contains exactly one compact JSON response followed by a newline for submission or cancellation;
- diagnostics and TUI rendering use stderr, never stdout;
- inherited stdin supplies interactive input;
- exit code `0` means approved or changes requested;
- exit code `2` means cancelled;
- exit code `1` means process, input, or protocol failure, in which case no response object is promised.

### Current-terminal Git review

```sh
clankerdiff review \
  --ui=tui \
  --tui-placement=current \
  --format=json \
  /path/to/repository
```

The historical external-terminal mode remains the default. It uses `CLANKERDIFF_TUI_COMMAND` and the private `attach` transport. That transport is not a public integration protocol.

An embedding terminal application is responsible for its own lifecycle: stop its input reader, restore cooked mode, spawn Clankerdiff with stdin and stderr inherited and stdout piped, wait for it, recreate its terminal/event reader, and force a complete redraw.

A shell has the required file-descriptor arrangement by default when only stdout is captured:

```sh
result_file=$(mktemp)
set +e
clankerdiff review --ui=tui --tui-placement=current --format=json . >"$result_file"
status=$?
set -e
case "$status" in
  0) echo "submitted: $(cat "$result_file")" ;;
  2) echo "cancelled: $(cat "$result_file")" ;;
  *) echo "review process failed" >&2 ;;
esac
rm -f "$result_file"
```

### Exact in-memory Markdown review

Keep stdin available for interactive controls by writing the exact snapshot to a temporary file. Use `--source-path` to retain its logical identity in comments and results:

```sh
snapshot=$(mktemp -t plan.XXXXXX.md)
printf '%s' "$PLAN_SOURCE" >"$snapshot"
clankerdiff markdown \
  --ui=tui \
  --format=json \
  --source-path docs/plans/next.md \
  --title "Next plan" \
  "$snapshot"
status=$?
rm -f "$snapshot"
exit "$status"
```

The physical temporary path is not placed into the document metadata when `--source-path` is supplied.

## Generic diff input

Hosts can replace an in-progress preview from complete text snapshots without a filesystem watcher or incremental Git transport:

```rust,ignore
let file = FileDiff::from_texts(repo_path, previous_source, current_source);
```

`RepoPath` accepts only normalized repository-relative paths. Embedders must strip the repository root and reject absolute or traversing paths rather than weakening this invariant.

## Development

```sh
just verify
```

The workspace MSRV is declared in the root `Cargo.toml`. Rust API compatibility follows package semver; the serialized Clankerdiff protocol has its own explicit version and compatibility policy.
