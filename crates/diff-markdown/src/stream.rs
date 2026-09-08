use std::{
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownStreamUpdate {
    pub revision: u64,
    pub changed_source: Range<usize>,
    pub reset: bool,
    pub finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarkdownStreamIdentity(u64);

impl Default for MarkdownStreamIdentity {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Default)]
pub struct MarkdownStream {
    source: String,
    revision: u64,
    identity: MarkdownStreamIdentity,
    finished: bool,
}

impl Clone for MarkdownStream {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            revision: self.revision,
            identity: MarkdownStreamIdentity::default(),
            finished: self.finished,
        }
    }
}

impl MarkdownStream {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> MarkdownStreamUpdate {
        if chunk.is_empty() && !self.finished {
            return self.unchanged();
        }
        let start = self.source.len();
        let reset = std::mem::take(&mut self.finished);
        self.source.push_str(chunk);
        self.update(start..self.source.len(), reset)
    }

    pub fn replace(&mut self, source: impl Into<String>) -> MarkdownStreamUpdate {
        self.source = source.into();
        self.finished = false;
        self.update(0..self.source.len(), true)
    }

    pub fn finish(&mut self) -> MarkdownStreamUpdate {
        if self.finished {
            return self.unchanged();
        }
        self.finished = true;
        self.update(self.source.len()..self.source.len(), false)
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
    pub const fn identity(&self) -> MarkdownStreamIdentity {
        self.identity
    }

    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    fn update(&mut self, changed_source: Range<usize>, reset: bool) -> MarkdownStreamUpdate {
        self.revision = self.revision.wrapping_add(1);
        MarkdownStreamUpdate {
            revision: self.revision,
            changed_source,
            reset,
            finished: self.finished,
        }
    }

    fn unchanged(&self) -> MarkdownStreamUpdate {
        MarkdownStreamUpdate {
            revision: self.revision,
            changed_source: self.source.len()..self.source.len(),
            reset: false,
            finished: self.finished,
        }
    }
}
