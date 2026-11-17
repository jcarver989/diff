use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use std::cmp::Reverse;

pub(crate) fn fuzzy_filter<T: AsRef<str>>(
    query: &str,
    keys: impl ExactSizeIterator<Item = T>,
    output: &mut Vec<usize>,
) {
    output.clear();
    if query.trim().is_empty() {
        output.extend(0..keys.len());
        return;
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut haystack = Vec::new();
    let mut scored: Vec<(u32, usize)> = keys
        .enumerate()
        .filter_map(|(index, key)| {
            let score = pattern.score(Utf32Str::new(key.as_ref(), &mut haystack), &mut matcher)?;
            Some((score, index))
        })
        .collect();
    scored.sort_by_key(|&(score, _)| Reverse(score));
    output.extend(scored.into_iter().map(|(_, index)| index));
}
