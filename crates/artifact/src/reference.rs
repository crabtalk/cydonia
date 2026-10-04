//! The written form that names a thing in a project: `[project]#ref[:from[-to]]`.
//!
//! | Written          | Names                                   |
//! | ---------------- | --------------------------------------- |
//! | `#43`            | entry 43                                |
//! | `#43:5-7`        | turns 5 to 7 of session #43             |
//! | `DEV-12`         | card DEV-12                             |
//! | `foo#43:5`       | turn 5 of session #43 in project `foo`  |
//! | `foo#DEV-12`     | card DEV-12 in project `foo`            |
//!
//! Each form may be written as a link behind a `cydonia://` scheme:
//! `cydonia://foo#43:5-7`.
//!
//! Parsing only. Which directory `foo` is, and whether `#43` is a session, are
//! the caller's to settle.

/// A parsed reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference<'a> {
    /// The project's name as written, before the `#`. Nothing means the
    /// project the reference was written in.
    pub project: Option<&'a str>,
    pub target: Target<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target<'a> {
    /// An entry by its number, and a run of its turns when one is named.
    Entry { number: u64, turns: Option<Turns> },
    /// A card by its board's key and its number on that board. The key is as
    /// written; keys match case-insensitively.
    Card { key: &'a str, handle: u64 },
}

/// A run of turns, counted from 1, both ends included. `from <= to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turns {
    pub from: u64,
    pub to: u64,
}

impl std::fmt::Display for Turns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.from == self.to {
            true => write!(f, "{}", self.from),
            false => write!(f, "{}-{}", self.from, self.to),
        }
    }
}

/// The scheme an entry link is written behind.
pub const SCHEME: &str = "cydonia://";

/// Read one reference, or nothing for text that is not one. Surrounding
/// whitespace is not trimmed.
pub fn parse(text: &str) -> Option<Reference<'_>> {
    let text = text.strip_prefix(SCHEME).unwrap_or(text);
    let Some((project, rest)) = text.split_once('#') else {
        let (key, handle) = card(text)?;
        return Some(Reference {
            project: None,
            target: Target::Card { key, handle },
        });
    };
    let project = match project {
        "" => None,
        name if name.chars().any(|c| c.is_whitespace() || c == ':') => return None,
        name => Some(name),
    };
    let target = match rest.split_once(':') {
        Some((number, turns)) => Target::Entry {
            number: positive(number)?,
            turns: Some(range(turns)?),
        },
        None => match positive(rest) {
            Some(number) => Target::Entry {
                number,
                turns: None,
            },
            None => {
                let (key, handle) = card(rest)?;
                Target::Card { key, handle }
            }
        },
    };
    Some(Reference { project, target })
}

/// A reference still being typed: the project as far as it is written, and
/// the start of what comes after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partial<'a> {
    /// Before a `#`. Nothing means no `#` has been typed, or it opens the text.
    pub project: Option<&'a str>,
    pub target: Prefix<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prefix<'a> {
    /// The leading digits of an entry's number, empty right after a `#`.
    Number(&'a str),
    /// The leading letters and digits of a board's key, before any `-`.
    Key(&'a str),
}

/// Read the start of a reference: `#`, `#4`, `foo#`, `foo#4`, `4`, `DE`.
/// Nothing for text no reference starts with. A bare key prefix carries no
/// project; a bare number is an entry's.
pub fn partial(text: &str) -> Option<Partial<'_>> {
    let text = text.strip_prefix(SCHEME).unwrap_or(text);
    let (project, rest) = match text.split_once('#') {
        Some(("", rest)) => (None, rest),
        Some((name, _)) if name.chars().any(|c| c.is_whitespace() || c == ':') => return None,
        Some((name, rest)) => (Some(name), rest),
        None => (None, text),
    };
    let target = if rest.bytes().all(|b| b.is_ascii_digit()) {
        Prefix::Number(rest)
    } else if rest.chars().all(|c| c.is_ascii_alphanumeric()) {
        Prefix::Key(rest)
    } else {
        return None;
    };
    Some(Partial { project, target })
}

/// `KEY-N`, split at the last `-`: a key may end in a digit.
fn card(text: &str) -> Option<(&str, u64)> {
    let (key, handle) = text.rsplit_once('-')?;
    let keyed = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric());
    keyed.then_some(())?;
    Some((key, positive(handle)?))
}

/// `5` or `5-7`.
fn range(text: &str) -> Option<Turns> {
    let (from, to) = match text.split_once('-') {
        Some((from, to)) => (positive(from)?, positive(to)?),
        None => {
            let one = positive(text)?;
            (one, one)
        }
    };
    (from <= to).then_some(Turns { from, to })
}

/// Digits only — no sign, no spaces — and above zero.
fn positive(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|n| *n > 0)
}
