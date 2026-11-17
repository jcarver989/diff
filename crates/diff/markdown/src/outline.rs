use crate::{MarkdownBlockKind, MarkdownDocument, MarkdownTargetId, SourceRange};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownHeading {
    pub level: u8,
    pub title: String,
    pub source: SourceRange,
    pub target_id: MarkdownTargetId,
}

#[must_use]
pub fn outline(document: &MarkdownDocument) -> Vec<MarkdownHeading> {
    headings(document)
        .map(|(level, title, source, target_id)| MarkdownHeading {
            level,
            title: title.to_owned(),
            source: source.clone(),
            target_id,
        })
        .collect()
}

pub(crate) fn headings(
    document: &MarkdownDocument,
) -> impl Iterator<Item = (u8, &str, &SourceRange, MarkdownTargetId)> {
    document.blocks().iter().filter_map(|block| match &block.kind {
        MarkdownBlockKind::Heading { level, content } => {
            Some((*level, content.text(), &block.source, block.target_id?))
        }
        _ => None,
    })
}

pub(crate) fn heading_path(document: &MarkdownDocument, line: usize) -> Vec<String> {
    let mut path = Vec::<(u8, &str)>::new();
    for (level, title, source, _) in headings(document) {
        if source.lines.start > line {
            break;
        }
        while path.last().is_some_and(|(previous, _)| *previous >= level) {
            path.pop();
        }
        path.push((level, title));
    }
    path.into_iter().map(|(_, title)| title.to_owned()).collect()
}
