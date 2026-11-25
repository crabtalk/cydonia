//! Notes filed against a range of an article: `comments.toml` beside its
//! `content.md`.
//!
//! A range is byte offsets into `content.md` as it was when the file was last
//! written, and `content` is a hash of that text. Every write of `content.md`
//! through `document::write` maps the ranges through the change. A
//! file whose hash does not match the document is re-found by each comment's
//! quote and the text around it.

pub use crate::comment::{Author, Reply};
use crate::{comment::Thread, project};
use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::{
    ops::Range,
    path::{Path, PathBuf},
};

pub const FILE: &str = "comments.toml";

/// Bytes of text kept on each side of a quote.
const CONTEXT: usize = 32;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comments {
    /// The hash of the `content.md` the ranges index into.
    #[serde(default)]
    pub content: String,
    #[serde(default, rename = "comment", skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub start: usize,
    pub end: usize,
    /// The text the range held when the comment was filed.
    pub quote: String,
    #[serde(default)]
    pub before: String,
    #[serde(default)]
    pub after: String,
    #[serde(flatten)]
    pub thread: Thread,
}

impl std::ops::Deref for Comment {
    type Target = Thread;

    fn deref(&self) -> &Thread {
        &self.thread
    }
}

impl std::ops::DerefMut for Comment {
    fn deref_mut(&mut self) -> &mut Thread {
        &mut self.thread
    }
}

impl Comment {
    /// A comment on `range` of `text`, quoting it.
    pub fn new(
        id: u64,
        text: &str,
        range: Range<usize>,
        note: String,
        turn: Option<String>,
    ) -> Self {
        let before = floor(text, range.start.saturating_sub(CONTEXT));
        let after = ceil(text, (range.end + CONTEXT).min(text.len()));
        Self {
            start: range.start,
            end: range.end,
            quote: text[range.clone()].to_owned(),
            before: text[before..range.start].to_owned(),
            after: text[range.end..after].to_owned(),
            thread: Thread::new(id, note, turn),
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    pub fn opens(&self) -> bool {
        !self.resolved
    }

    /// Whether the words it was on are gone.
    pub fn detached(&self) -> bool {
        self.start >= self.end
    }
}

impl Comments {
    /// The comment `id`, to change.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Comment> {
        self.comments.iter_mut().find(|comment| comment.id == id)
    }

    /// Take the replies `from` holds past the ones each comment here already
    /// has: what was said in a thread since these were read.
    pub fn take_replies(&mut self, from: &Comments) {
        for held in &mut self.comments {
            let Some(there) = from.comments.iter().find(|there| there.id == held.id) else {
                continue;
            };
            if there.replies.len() > held.replies.len() && there.replies.starts_with(&held.replies)
            {
                held.replies = there.replies.clone();
            }
        }
    }

    /// Hold the ranges against `text`.
    pub fn stamp(&mut self, text: &str) {
        self.content = project::version(text.as_bytes());
    }

    /// Whether the ranges index into `text`.
    pub fn fits(&self, text: &str) -> bool {
        self.content == project::version(text.as_bytes())
    }

    /// Ranges into `new`, from wherever they were held: through a diff where
    /// they were held against `old`, kept where they already index into `new`,
    /// else re-found by quote.
    pub fn carry(&mut self, old: Option<&str>, new: &str) {
        match old {
            Some(old) if self.fits(old) => self.remap(old, new),
            _ if self.fits(new) => {}
            _ => self.refind(new),
        }
    }

    /// Carry every range from `old` to `new` through a diff of the two, and
    /// hold them against `new`.
    ///
    /// An end inside text that went lands where it was; an end at an insertion
    /// lands after it. A range whose text all went comes out detached.
    pub fn remap(&mut self, old: &str, new: &str) {
        let ops = capture_diff_slices(Algorithm::Myers, old.as_bytes(), new.as_bytes());
        for comment in &mut self.comments {
            let start = ceil(new, map(&ops, comment.start, new.len()));
            let end = ceil(new, map(&ops, comment.end, new.len())).max(start);
            comment.start = start;
            comment.end = end;
        }
        self.stamp(new);
    }

    /// Find each quote in `text`: the match with the most of its context
    /// around it, then the nearest to where it was. A quote not found comes out
    /// detached where it was, clamped to the text.
    pub fn refind(&mut self, text: &str) {
        for comment in &mut self.comments {
            let found = match comment.quote.is_empty() {
                true => None,
                false => text
                    .match_indices(comment.quote.as_str())
                    .map(|(at, _)| at)
                    .max_by_key(|at| {
                        let fit = suffix(&text[..*at], &comment.before)
                            + prefix(&text[at + comment.quote.len()..], &comment.after);
                        (fit, std::cmp::Reverse(at.abs_diff(comment.start)))
                    }),
            };
            match found {
                Some(at) => {
                    comment.start = at;
                    comment.end = at + comment.quote.len();
                }
                None => {
                    let at = floor(text, comment.start.min(text.len()));
                    comment.start = at;
                    comment.end = at;
                }
            }
        }
        self.stamp(text);
    }

    /// An id no comment here has, from `now`.
    pub fn fresh_id(&self, now: u64) -> u64 {
        Thread::fresh_id(now, self.comments.iter().map(|comment| comment.id))
    }
}

/// The comments file beside a `content.md`.
pub fn path(content: &Path) -> PathBuf {
    content.with_file_name(FILE)
}

/// The comments beside `content` as they were written, held against whatever
/// text their hash names. Empty for an article with none, or a file that does
/// not parse.
#[cfg(feature = "fs")]
pub fn read(content: &Path) -> Comments {
    parse(&std::fs::read_to_string(path(content)).unwrap_or_default())
}

/// Write `comments`, held against `text`, beside `content`. No comments
/// removes the file.
#[cfg(feature = "fs")]
pub fn save(content: &Path, text: &str, comments: &Comments) -> std::io::Result<()> {
    let path = path(content);
    if comments.comments.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let mut comments = comments.clone();
    comments.stamp(text);
    crate::fs::save(&path, serialize(&comments))
}

pub fn parse(text: &str) -> Comments {
    toml::from_str(text).unwrap_or_default()
}

pub fn serialize(comments: &Comments) -> String {
    toml::to_string(comments).unwrap_or_default()
}

/// Where `at` in the old text lands in the new.
fn map(ops: &[DiffOp], at: usize, end: usize) -> usize {
    for op in ops {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } if (old_index..old_index + len).contains(&at) => return new_index + (at - old_index),
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } if (old_index..old_index + old_len).contains(&at) => return new_index,
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                ..
            } if (old_index..old_index + old_len).contains(&at) => return new_index,
            _ => {}
        }
    }
    end
}

/// How much of `want`'s end `text` ends with.
fn suffix(text: &str, want: &str) -> usize {
    text.bytes()
        .rev()
        .zip(want.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count()
}

/// How much of `want`'s start `text` starts with.
fn prefix(text: &str, want: &str) -> usize {
    text.bytes()
        .zip(want.bytes())
        .take_while(|(a, b)| a == b)
        .count()
}

fn floor(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn ceil(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}
