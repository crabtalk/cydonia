//! A board's short name, the prefix every card handle carries: `ROAD` in
//! `ROAD-12`.
//!
//! Derived from the name when the board is made and kept across a rename — a
//! handle already written down has to go on meaning the card it meant. ASCII
//! only, since a handle is typed.

use std::collections::HashSet;

/// What a name with nothing ASCII in it is keyed as, before uniquing.
const FALLBACK: &str = "B";

/// Initials for a name of several words, the opening of a name of one.
const INITIALS: usize = 4;
const OPENING: usize = 3;

/// A key as typed: its ASCII alphanumerics, uppercased. Nothing for a string
/// with none.
pub fn normalize(raw: &str) -> Option<String> {
    let key: String = raw
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_uppercase();
    (!key.is_empty()).then_some(key)
}

/// A key for a board of this name, clear of the ones its neighbours hold:
/// `Why CrabTalk` → `WC`, `Roadmap` → `ROA`. Unique by a trailing digit, which
/// is why a handle splits off its *last* `-` — `ROA2-5` is a card on the second
/// Roadmap, not card `2-5` on `ROA`.
pub fn derive(name: &str, taken: &HashSet<String>) -> String {
    let words: Vec<&str> = name
        .split_whitespace()
        .filter(|word| word.chars().any(|c| c.is_ascii_alphanumeric()))
        .collect();
    let base = match words.len() >= 2 {
        true => words
            .iter()
            .filter_map(|word| word.chars().find(char::is_ascii_alphanumeric))
            .take(INITIALS)
            .collect::<String>(),
        false => name
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(OPENING)
            .collect::<String>(),
    };
    let base = match base.is_empty() {
        true => FALLBACK.to_owned(),
        false => base.to_uppercase(),
    };
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or(base)
}
