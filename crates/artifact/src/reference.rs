//! The written form that names a thing in a project:
//! `[project]#ref[:from[-to] | #anchor]`.
//!
//! | Written          | Names                                        |
//! | ---------------- | -------------------------------------------- |
//! | `#43`            | entry 43                                     |
//! | `#43:5-7`        | turns 5 to 7 of session #43                  |
//! | `#12:5-7`        | lines 5 to 7 of article #12                  |
//! | `#12#setup`      | the section under article #12's `setup`      |
//! | `DEV-12`         | card DEV-12                                  |
//! | `foo#43:5`       | turn 5 of session #43 in project `foo`       |
//! | `foo#DEV-12`     | card DEV-12 in project `foo`                 |
//!
//! Each form may be written as a link behind a `cydonia://` scheme:
//! `cydonia://foo#43:5-7`.
//!
//! Parsing only. Which directory `foo` is, what kind of entry `#43` is, and
//! whether a span or an anchor fits that kind, are the caller's to settle.

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
    /// An entry by its number, and the part of it named after the number.
    Entry {
        number: u64,
        within: Option<Within<'a>>,
    },
    /// A card by its board's key and its number on that board. The key is as
    /// written; keys match case-insensitively.
    Card { key: &'a str, handle: u64 },
}

/// The part of an entry a reference names after its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Within<'a> {
    /// `:5-7` — a session's turns, an article's lines.
    Span(Span),
    /// `#setup` — an article's heading, by its anchor (see
    /// [`crate::article::anchor`]), and the section under it.
    Heading(&'a str),
}

/// A run counted from 1, both ends included. `from <= to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: u64,
    pub to: u64,
}

impl std::fmt::Display for Span {
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
    let target = if let Some((number, anchor)) = rest.split_once('#') {
        Target::Entry {
            number: positive(number)?,
            within: Some(Within::Heading(heading(anchor)?)),
        }
    } else if let Some((number, span)) = rest.split_once(':') {
        Target::Entry {
            number: positive(number)?,
            within: Some(Within::Span(range(span)?)),
        }
    } else if let Some(number) = positive(rest) {
        Target::Entry {
            number,
            within: None,
        }
    } else {
        let (key, handle) = card(rest)?;
        Target::Card { key, handle }
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

/// An anchor as [`crate::article::anchor`] writes one: letters, digits, `-`
/// and `_`.
fn heading(text: &str) -> Option<&str> {
    let anchored = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    anchored.then_some(text)
}

/// `5` or `5-7`.
fn range(text: &str) -> Option<Span> {
    let (from, to) = match text.split_once('-') {
        Some((from, to)) => (positive(from)?, positive(to)?),
        None => {
            let one = positive(text)?;
            (one, one)
        }
    };
    (from <= to).then_some(Span { from, to })
}

/// Digits only — no sign, no spaces — and above zero.
fn positive(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|n| *n > 0)
}
