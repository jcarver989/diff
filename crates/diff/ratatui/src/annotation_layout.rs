use crate::annotation::{AnnotationBox, AnnotationKind};
use std::{collections::BTreeMap, ops::Range};

#[derive(Debug, Clone, Copy)]
pub(crate) enum AnnotationRow<'a> {
    Source(usize),
    Annotation {
        source: usize,
        annotation: &'a AnnotationBox,
        line: usize,
    },
}

#[derive(Debug, Default)]
pub(crate) struct AnnotationLayout {
    range: Range<usize>,
    annotations: Vec<(usize, usize, AnnotationBox)>,
    len: usize,
}

impl AnnotationLayout {
    pub(crate) fn new(range: Range<usize>, anchored: BTreeMap<usize, Vec<AnnotationBox>>) -> Self {
        let mut annotations = Vec::new();
        let mut extra = 0;
        for (source, boxes) in anchored {
            if !range.contains(&source) {
                continue;
            }
            for annotation in boxes {
                let visual = source - range.start + 1 + extra;
                extra += annotation.lines().len();
                annotations.push((source, visual, annotation));
            }
        }
        let len = range.len() + extra;
        Self {
            range,
            annotations,
            len,
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }
    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn visual_offset_for_source(&self, source: usize) -> Option<usize> {
        self.range.contains(&source).then(|| {
            source - self.range.start
                + self
                    .annotations
                    .iter()
                    .take_while(|(anchor, _, _)| *anchor < source)
                    .map(|(_, _, annotation)| annotation.lines().len())
                    .sum::<usize>()
        })
    }

    pub(crate) fn focused_visual_row(&self, source: usize, draft: bool) -> Option<usize> {
        if draft {
            for (anchor, visual, annotation) in &self.annotations {
                if *anchor == source && annotation.kind() == AnnotationKind::Draft {
                    return Some(visual + annotation.cursor_line().unwrap_or(0));
                }
            }
        }
        self.visual_offset_for_source(source)
    }

    pub(crate) fn row(&self, visual: usize) -> Option<AnnotationRow<'_>> {
        if visual >= self.len {
            return None;
        }
        let mut extra = 0;
        for (source, start, annotation) in &self.annotations {
            if visual < *start {
                break;
            }
            if visual < start + annotation.lines().len() {
                return Some(AnnotationRow::Annotation {
                    source: *source,
                    annotation,
                    line: visual - start,
                });
            }
            extra += annotation.lines().len();
        }
        Some(AnnotationRow::Source(self.range.start + visual - extra))
    }
}
