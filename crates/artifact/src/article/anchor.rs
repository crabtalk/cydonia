//! The part of an article's markdown a reference names after its number — see
//! [`crate::reference::Within`]: a run of its lines, or the section under a
//! heading.
//!
//! Byte ranges into the markdown as it is stored, which is what `article_read`
//! answers and what its line numbers count.

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::reference::Span;

/// The extensions bezel's markdown parser reads with. A heading is only a
/// heading here if it is one there.
const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_GFM);

/// The anchor GitHub gives a heading: lowercased, punctuation dropped but for
/// `-` and `_`, each space a `-`. The same rule as bezel's
/// `markdown::link::slug`, which follows `#anchor` links inside a document.
pub fn slug(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect()
}

/// A heading in the markdown: its anchor, its text, its level, and where it
/// starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub anchor: String,
    pub text: String,
    pub level: u8,
    pub start: usize,
}

/// Every heading, in order. A slug an earlier heading already took gets `-1`,
/// `-2`, … as on GitHub.
pub fn headings(markdown: &str) -> Vec<Heading> {
    let mut found: Vec<Heading> = Vec::new();
    let mut open: Option<(u8, usize, String)> = None;
    for (event, range) in Parser::new_ext(markdown, OPTIONS).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                open = Some((depth(level), range.start, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                let Some((level, start, text)) = open.take() else {
                    continue;
                };
                let base = slug(&text);
                let anchor = std::iter::once(base.clone())
                    .chain((1..).map(|n| format!("{base}-{n}")))
                    .find(|anchor| found.iter().all(|heading| &heading.anchor != anchor))
                    .expect("an unbounded run of suffixes");
                found.push(Heading {
                    anchor,
                    text,
                    level,
                    start,
                });
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, open)) = &mut open {
                    open.push_str(&text);
                }
            }
            _ => {}
        }
    }
    found
}

fn depth(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// The section under the heading `anchor` names: from the heading to the next
/// one at its level or above, or to the end. Its heading alongside.
pub fn section(markdown: &str, anchor: &str) -> Option<(Heading, Range<usize>)> {
    let headings = headings(markdown);
    let at = headings
        .iter()
        .position(|heading| heading.anchor == anchor)?;
    let heading = headings[at].clone();
    let end = headings[at + 1..]
        .iter()
        .find(|next| next.level <= heading.level)
        .map_or(markdown.len(), |next| next.start);
    let start = heading.start;
    Some((heading, start..end))
}

/// Lines `span`, whole and with their line breaks. A span running past the
/// last line stops there; one starting past it is nothing.
pub fn lines(markdown: &str, span: Span) -> Option<Range<usize>> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(markdown.match_indices('\n').map(|(at, _)| at + 1))
        .filter(|&at| at < markdown.len())
        .collect();
    let from = usize::try_from(span.from).ok()?.checked_sub(1)?;
    let start = *starts.get(from)?;
    let end = usize::try_from(span.to)
        .ok()
        .and_then(|to| starts.get(to).copied())
        .unwrap_or(markdown.len());
    Some(start..end)
}

/// The lines a non-empty byte range of the markdown covers — [`lines`] the
/// other way.
pub fn span_of(markdown: &str, range: Range<usize>) -> Span {
    let line = |at: usize| markdown[..at].matches('\n').count() as u64 + 1;
    Span {
        from: line(range.start),
        to: line(range.end.max(range.start + 1) - 1),
    }
}
