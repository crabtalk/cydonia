//! Labels: names an entry carries, several at once, shared across projects.
//!
//! A label exists while at least one entry carries it; there is no list of
//! labels apart from the entries. Stored as `labels = ["research", "q3"]` on
//! the entry.

/// `text` as a label: lowercase and trimmed, with each run of whitespace as
/// one `-`. `None` for text with nothing in it.
pub fn normalize(text: &str) -> Option<String> {
    let name = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    (!name.is_empty()).then_some(name)
}

/// Every name in `names` normalised, sorted, each once.
pub fn normalize_all<S: AsRef<str>>(names: impl IntoIterator<Item = S>) -> Vec<String> {
    let mut labels: Vec<String> = names
        .into_iter()
        .filter_map(|name| normalize(name.as_ref()))
        .collect();
    labels.sort();
    labels.dedup();
    labels
}
