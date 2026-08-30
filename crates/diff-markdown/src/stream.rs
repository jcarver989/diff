use std::ops::Range;

/// Opaque information about a top-level fenced block continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FenceContinuation {
    character: u8,
    length: usize,
    opening: usize,
    content_start: usize,
    info: String,
}

impl FenceContinuation {
    /// Byte offset of the opening fence line.
    #[must_use]
    pub const fn opening_line(&self) -> usize {
        self.opening
    }

    #[must_use]
    pub const fn content_start(&self) -> usize {
        self.content_start
    }

    #[must_use]
    pub fn info_string(&self) -> &str {
        &self.info
    }

    #[must_use]
    pub fn find_close(&self, segment: &str) -> Option<Range<usize>> {
        let mut offset = 0;
        for line in segment.split_inclusive('\n') {
            let body = line.trim_end_matches(['\n', '\r']);
            if is_closing_fence(body, self.character, self.length) {
                return Some(offset..offset + line.len());
            }
            offset += line.len();
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownStreamUpdate {
    pub revision: u64,
    pub newly_stable: Range<usize>,
    pub unstable_tail: Range<usize>,
    pub reset: bool,
}

/// Append-oriented Markdown source tracker.
///
/// Prose stabilizes only at blank-line boundaries. Complete lines in a
/// top-level open fence stabilize incrementally, while contained fences and a
/// closing fence remain in the unstable tail until the following safe boundary.
#[derive(Debug, Clone, Default)]
pub struct MarkdownStream {
    source: String,
    revision: u64,
    generation: u64,
    stable: usize,
    continuation: Option<FenceContinuation>,
    finished: bool,
    /// Byte offset of the first line not yet scanned; appends resume here
    /// instead of rescanning the whole source.
    scanned: usize,
    /// Open fence state at `scanned`, with whether the fence is top-level.
    fence: Option<(FenceContinuation, bool)>,
}

impl MarkdownStream {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> MarkdownStreamUpdate {
        if self.finished {
            let mut source = std::mem::take(&mut self.source);
            source.push_str(chunk);
            return self.replace(source);
        }
        self.source.push_str(chunk);
        self.revision = self.revision.wrapping_add(1);
        self.recompute(false)
    }

    pub fn replace(&mut self, source: impl Into<String>) -> MarkdownStreamUpdate {
        self.source = source.into();
        self.revision = self.revision.wrapping_add(1);
        self.generation = self.generation.wrapping_add(1);
        self.finished = false;
        self.recompute(true)
    }

    pub fn finish(&mut self) -> MarkdownStreamUpdate {
        self.finished = true;
        self.revision = self.revision.wrapping_add(1);
        let old = self.stable;
        self.stable = self.source.len();
        self.continuation = None;
        MarkdownStreamUpdate {
            revision: self.revision,
            newly_stable: old..self.stable,
            unstable_tail: self.stable..self.source.len(),
            reset: false,
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn stable_offset(&self) -> usize {
        self.stable
    }
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    #[must_use]
    pub const fn continuation(&self) -> Option<&FenceContinuation> {
        self.continuation.as_ref()
    }

    fn recompute(&mut self, reset: bool) -> MarkdownStreamUpdate {
        let old = self.stable;
        if reset {
            self.scanned = 0;
            self.fence = None;
            self.stable = 0;
        }
        let mut stable = self.stable;
        let mut fence = self.fence.take();
        let mut offset = self.scanned;

        for line in self.source[offset..].split_inclusive('\n') {
            if !line.ends_with('\n') {
                break;
            }
            let body = line.trim_end_matches(['\n', '\r']);
            let end = offset + line.len();
            if let Some((open, top_level)) = &fence {
                if is_closing_fence(body, open.character, open.length) {
                    fence = None;
                    // Keep the close with the tail so final block spacing can
                    // still be determined by subsequent source.
                } else if *top_level {
                    stable = end;
                }
            } else if let Some(open) = opening_fence(body) {
                let continuation = FenceContinuation {
                    character: open.character,
                    length: open.length,
                    opening: offset,
                    content_start: end,
                    info: open.info,
                };
                if open.top_level {
                    stable = end;
                }
                fence = Some((continuation, open.top_level));
            } else if body.trim().is_empty() {
                stable = end;
            }
            offset = end;
        }

        self.scanned = offset;
        self.continuation = fence
            .as_ref()
            .filter(|(_, top_level)| *top_level)
            .map(|(continuation, _)| continuation.clone());
        self.fence = fence;
        self.stable = stable.min(self.source.len());
        MarkdownStreamUpdate {
            revision: self.revision,
            newly_stable: if reset {
                0..self.stable
            } else {
                old.min(self.stable)..self.stable
            },
            unstable_tail: self.stable..self.source.len(),
            reset,
        }
    }
}

struct OpeningFence {
    character: u8,
    length: usize,
    top_level: bool,
    info: String,
}

fn opening_fence(line: &str) -> Option<OpeningFence> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    let trimmed = &line[indent..];
    let contained = trimmed.starts_with('>')
        || trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || starts_ordered_item(trimmed);
    let candidate = if contained {
        trimmed
            .trim_start_matches('>')
            .trim_start()
            .strip_prefix("- ")
            .or_else(|| {
                trimmed
                    .trim_start_matches('>')
                    .trim_start()
                    .strip_prefix("* ")
            })
            .or_else(|| {
                trimmed
                    .trim_start_matches('>')
                    .trim_start()
                    .strip_prefix("+ ")
            })
            .unwrap_or_else(|| {
                let after_digits =
                    trimmed.trim_start_matches(|character: char| character.is_ascii_digit());
                after_digits.strip_prefix(". ").unwrap_or(trimmed)
            })
    } else {
        trimmed
    };
    let character = *candidate.as_bytes().first()?;
    if character != b'`' && character != b'~' {
        return None;
    }
    let length = candidate
        .bytes()
        .take_while(|byte| *byte == character)
        .count();
    if length < 3 {
        return None;
    }
    let info = &candidate[length..];
    if character == b'`' && info.contains('`') {
        return None;
    }
    Some(OpeningFence {
        character,
        length,
        top_level: !contained && indent <= 3,
        info: info.trim().to_owned(),
    })
}

fn is_closing_fence(line: &str, character: u8, opening_length: usize) -> bool {
    let trimmed = line.trim_start_matches(' ');
    if line.len().saturating_sub(trimmed.len()) > 3 {
        return false;
    }
    let length = trimmed
        .bytes()
        .take_while(|byte| *byte == character)
        .count();
    length >= opening_length && trimmed[length..].trim().is_empty()
}

fn starts_ordered_item(line: &str) -> bool {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(". ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_and_finish_are_utf8_safe() {
        let mut stream = MarkdownStream::new();
        stream.push("hello\n\n");
        stream.push("世界");
        assert_eq!(stream.source(), "hello\n\n世界");
        assert_eq!(stream.stable_offset(), 7);
        let update = stream.finish();
        assert_eq!(update.unstable_tail, 13..13);
    }

    #[test]
    fn top_level_fence_stabilizes_content_but_holds_close() {
        let mut stream = MarkdownStream::new();
        stream.push("```rust\nlet x = 1;\n```\n");
        assert_eq!(stream.stable_offset(), "```rust\nlet x = 1;\n".len());
        assert!(stream.continuation().is_none());
        stream.push("\n");
        assert_eq!(stream.stable_offset(), stream.source().len());
    }

    #[test]
    fn continuation_preserves_offsets_info_and_locates_its_close() {
        let mut stream = MarkdownStream::new();
        stream.push("~~~~rust {#example .numberLines}\r\nlet x = 1;\r\n");
        let continuation = stream.continuation().unwrap();
        assert_eq!(continuation.opening_line(), 0);
        assert_eq!(
            continuation.content_start(),
            "~~~~rust {#example .numberLines}\r\n".len()
        );
        assert_eq!(continuation.info_string(), "rust {#example .numberLines}");
        assert_eq!(
            continuation.find_close("code\n~~~\n~~~~\nafter\n"),
            Some(9..14)
        );
        assert_eq!(continuation.find_close("code\n~~~\n"), None);
    }

    #[test]
    fn replacement_resets_continuation_and_scanner() {
        let mut stream = MarkdownStream::new();
        stream.push("```rust\nopen\n");
        assert!(stream.continuation().is_some());
        let update = stream.replace("plain\n\n");
        assert!(update.reset);
        assert!(stream.continuation().is_none());
        assert_eq!(stream.stable_offset(), stream.source().len());
    }

    #[test]
    fn longer_fence_and_contained_fence_are_conservative() {
        let mut stream = MarkdownStream::new();
        stream.push("````\n```\n````\n");
        assert_eq!(stream.stable_offset(), "````\n```\n".len());
        let mut contained = MarkdownStream::new();
        contained.push("> ```\n> code\n");
        assert_eq!(contained.stable_offset(), 0);
        assert!(contained.continuation().is_none());
    }
}
