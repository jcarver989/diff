use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use std::cmp::Reverse;

#[must_use]
pub fn fuzzy_filter<T: AsRef<str>>(query: &str, keys: impl IntoIterator<Item = T>) -> Vec<usize> {
    let keys = keys.into_iter();
    if query.trim().is_empty() {
        return (0..keys.count()).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = keys
        .enumerate()
        .filter_map(|(index, key)| {
            pattern.score(Utf32Str::new(key.as_ref(), &mut buf), &mut matcher).map(|score| (score, index))
        })
        .collect();
    scored.sort_by_key(|&(score, _)| Reverse(score));
    scored.into_iter().map(|(_, index)| index).collect()
}
