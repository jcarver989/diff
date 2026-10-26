use super::{Hunk, PatchLine, PatchLineKind};
use similar::{ChangeTag, TextDiff};
use std::ops::Range;

/// Derives grouped hunks and the incomplete-final-line flag from complete texts.
pub(super) fn derive_patch(old: &str, new: &str) -> (Vec<Hunk>, bool) {
    let mut lines = Vec::new();
    let (mut old_line, mut new_line) = (0, 0);
    for change in TextDiff::from_lines(old, new).iter_all_changes() {
        let text = strip_line_ending(change.value());
        lines.push(match change.tag() {
            ChangeTag::Equal => {
                old_line += 1;
                new_line += 1;
                PatchLine::context(text, old_line, new_line)
            }
            ChangeTag::Delete => {
                old_line += 1;
                PatchLine::removed(text, old_line)
            }
            ChangeTag::Insert => {
                new_line += 1;
                PatchLine::added(text, new_line)
            }
        });
    }

    let changed = lines
        .iter()
        .any(|line| matches!(line.kind, PatchLineKind::Added | PatchLineKind::Removed));
    let mut hunks = if changed {
        grouped_hunks(&lines, 3)
    } else {
        Vec::new()
    };

    let old_incomplete = !old.is_empty() && !old.ends_with('\n');
    let new_incomplete = !new.is_empty() && !new.ends_with('\n');
    if let Some(hunk) = hunks.last_mut() {
        if new_incomplete {
            mark_last_incomplete(hunk, PatchLineKind::Added);
        }
        if old_incomplete {
            mark_last_incomplete(hunk, PatchLineKind::Removed);
        }
    }
    (hunks, old_incomplete || new_incomplete)
}

fn strip_line_ending(value: &str) -> &str {
    value.strip_suffix('\n').map_or(value, |trimmed| {
        trimmed.strip_suffix('\r').unwrap_or(trimmed)
    })
}

fn grouped_hunks(lines: &[PatchLine], context: usize) -> Vec<Hunk> {
    let mut ranges = Vec::<Range<usize>>::new();
    for index in lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (line.kind != PatchLineKind::Context).then_some(index))
    {
        let start = index.saturating_sub(context);
        let end = index.saturating_add(context + 1).min(lines.len());
        if let Some(previous) = ranges.last_mut()
            && start <= previous.end
        {
            previous.end = previous.end.max(end);
        } else {
            ranges.push(start..end);
        }
    }
    ranges
        .into_iter()
        .map(|range| whole_file_hunk(lines[range].to_vec()))
        .collect()
}

fn whole_file_hunk(lines: Vec<PatchLine>) -> Hunk {
    let old_start = lines.iter().find_map(|line| line.old_line_no).unwrap_or(0);
    let new_start = lines.iter().find_map(|line| line.new_line_no).unwrap_or(0);
    let old_count = lines
        .iter()
        .filter(|line| line.old_line_no.is_some())
        .count();
    let new_count = lines
        .iter()
        .filter(|line| line.new_line_no.is_some())
        .count();
    Hunk {
        header: format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@"),
        function_context: None,
        old_start,
        old_count,
        new_start,
        new_count,
        lines,
    }
}

fn mark_last_incomplete(hunk: &mut Hunk, kind: PatchLineKind) {
    if let Some(line) = hunk.lines.iter_mut().rev().find(|line| line.kind == kind) {
        line.no_newline = true;
    }
}
