//! Map patch lines to syntax tokens from complete source snapshots.

use super::{Area, Change, PATCH_LIMIT, command, output};
use bezel::theme::HighlightKind;
use std::{
    collections::HashSet,
    ffi::OsString,
    io::Read as _,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

type Spans = Vec<(Range<usize>, HighlightKind)>;
const LINE_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Added,
    Removed,
    Context,
    Hunk,
    Meta,
}

pub struct Line {
    pub kind: Kind,
    pub old: Option<usize>,
    pub new: Option<usize>,
    pub text: String,
    /// Byte ranges in `text`, after expanding tabs and limiting long lines.
    pub spans: Spans,
}

#[derive(Default)]
pub struct Preview {
    pub patch: String,
    pub lines: Vec<Line>,
    path: PathBuf,
    old: Option<String>,
    new: Option<String>,
}

impl Preview {
    pub fn plain(patch: String) -> Arc<Self> {
        Arc::new(Self::build(Path::new(""), patch, None, None))
    }

    /// Reuse highlighting only when the patch and both source snapshots match.
    pub fn load(root: &Path, file: &Change, patch: String, previous: Arc<Self>) -> Arc<Self> {
        let (old, new) = if language(&file.path).is_some() {
            versions(root, file)
        } else {
            (None, None)
        };
        if previous.path == file.path
            && previous.patch == patch
            && previous.old == old
            && previous.new == new
        {
            return previous;
        }
        Arc::new(Self::build(&file.path, patch, old, new))
    }

    /// Hide headers and folded hunks without changing the copyable patch.
    pub fn visible_rows(&self, collapsed: &HashSet<String>) -> Vec<usize> {
        let mut hidden = false;
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(ix, line)| {
                if line.is_hunk() {
                    hidden = collapsed.contains(&line.text);
                    return Some(ix);
                }
                (!hidden
                    && !line.is_patch_header()
                    && (line.kind != Kind::Meta || !line.text.is_empty()))
                .then_some(ix)
            })
            .collect()
    }

    pub fn hunk_label(&self, ix: usize) -> String {
        let line = &self.lines[ix];
        let Some(((old, old_count), (new, new_count))) = hunk(&line.text) else {
            return "Conflict changes".into();
        };
        let (start, count) = if new_count == 0 {
            (old, old_count)
        } else {
            (new, new_count)
        };
        let body = self.lines[ix + 1..]
            .iter()
            .take_while(|line| !line.is_hunk());
        let (mut added, mut removed) = (0, 0);
        for line in body {
            added += usize::from(line.kind == Kind::Added);
            removed += usize::from(line.kind == Kind::Removed);
        }
        let location = match count {
            0 | 1 => format!("Line {start}"),
            _ => format!("Lines {start}–{}", start + count - 1),
        };
        format!("{location}  ·  +{added} −{removed}")
    }

    pub fn summary(&self) -> &'static str {
        if self.patch.contains("rename from ") {
            "File renamed"
        } else if self.patch.contains("copy from ") {
            "File copied"
        } else if self.patch.contains("new file mode ") {
            "Empty file added"
        } else if self.patch.contains("deleted file mode ") {
            "Empty file deleted"
        } else if self.patch.contains("old mode ") {
            "File permissions changed"
        } else {
            "No textual changes"
        }
    }

    fn build(path: &Path, patch: String, old: Option<String>, new: Option<String>) -> Self {
        let language = language(path);
        let before = old.as_deref().map(|text| Source::new(text, language));
        let after = new.as_deref().map(|text| Source::new(text, language));
        let lines = lines(&patch, before.as_ref(), after.as_ref());
        Self {
            patch,
            lines,
            path: path.to_owned(),
            old,
            new,
        }
    }
}

impl Line {
    pub fn is_hunk(&self) -> bool {
        self.kind == Kind::Hunk || (self.kind == Kind::Meta && self.text.starts_with("@@@ "))
    }

    fn is_patch_header(&self) -> bool {
        self.kind == Kind::Meta
            && [
                "diff --git ",
                "diff --cc ",
                "diff --combined ",
                "index ",
                "--- ",
                "+++ ",
                "new file mode ",
                "deleted file mode ",
                "old mode ",
                "new mode ",
                "similarity index ",
                "dissimilarity index ",
                "rename from ",
                "rename to ",
                "copy from ",
                "copy to ",
            ]
            .iter()
            .any(|prefix| self.text.starts_with(prefix))
    }
}

fn language(path: &Path) -> Option<&'static str> {
    let tag = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_else(|| {
            match path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
            {
                ".bashrc" | ".bash_profile" | ".zshrc" | ".profile" => "sh",
                _ => "",
            }
        })
        .to_ascii_lowercase();
    let tag = match tag.as_str() {
        "mjs" | "cjs" => "js",
        "mts" | "cts" => "ts",
        other => other,
    };
    syntax::lang::resolve(tag).map(|language| language.name)
}

fn versions(root: &Path, file: &Change) -> (Option<String>, Option<String>) {
    let blob = |revision: &str, path: &Path| {
        let mut spec = OsString::from(revision);
        spec.push(path);
        let mut git = command(root);
        git.args(["cat-file", "blob"]).arg(spec);
        let (exit, bytes, truncated) = output(git).ok()?;
        (exit.success() && !truncated && !bytes.contains(&0))
            .then(|| String::from_utf8(bytes).ok())
            .flatten()
    };
    let working = || {
        let path = root.join(&file.path);
        // Symlink patches show a target path, not the target's source code.
        if !std::fs::symlink_metadata(&path).ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .ok()?
            .take(PATCH_LIMIT + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        (bytes.len() as u64 <= PATCH_LIMIT && !bytes.contains(&0))
            .then(|| String::from_utf8(bytes).ok())
            .flatten()
    };
    match file.area {
        Area::Staged => (
            blob("HEAD:", file.original.as_deref().unwrap_or(&file.path)),
            blob(":", &file.path),
        ),
        Area::Unstaged => (blob(":", &file.path), working()),
        Area::Untracked => (None, working()),
    }
}

struct Source<'a> {
    text: &'a str,
    offsets: Vec<Range<usize>>,
    spans: Spans,
}

impl<'a> Source<'a> {
    fn new(text: &'a str, language: Option<&str>) -> Self {
        let mut offset = 0;
        let offsets = text
            .split_inclusive('\n')
            .map(|line| {
                let start = offset;
                offset += line.len();
                // `str::lines`, used on the patch, removes a CRLF terminator too.
                let line = line.strip_suffix('\n').unwrap_or(line);
                let line = line.strip_suffix('\r').unwrap_or(line);
                start..start + line.len()
            })
            .collect();
        let spans = language
            .and_then(|language| syntax::highlight(text, language))
            .unwrap_or_default();
        Self {
            text,
            offsets,
            spans,
        }
    }

    fn spans(&self, number: usize, code: &str) -> Spans {
        let Some(range) = number.checked_sub(1).and_then(|ix| self.offsets.get(ix)) else {
            return Vec::new();
        };
        // Skip highlighting if the file changed after Git read it.
        if &self.text[range.clone()] != code {
            return Vec::new();
        }
        let start = self
            .spans
            .partition_point(|(span, _)| span.end <= range.start);
        self.spans[start..]
            .iter()
            .take_while(|(span, _)| span.start < range.end)
            .map(|(span, kind)| {
                (
                    span.start.max(range.start) - range.start
                        ..span.end.min(range.end) - range.start,
                    *kind,
                )
            })
            .collect()
    }
}

fn hunk(line: &str) -> Option<((usize, usize), (usize, usize))> {
    fn side(value: &str, prefix: char) -> Option<(usize, usize)> {
        let mut parts = value.strip_prefix(prefix)?.split(',');
        let start = parts.next()?.parse().ok()?;
        let count = parts.next().map(str::parse).transpose().ok()?.unwrap_or(1);
        Some((start, count))
    }
    let mut parts = line.strip_prefix("@@ ")?.split_whitespace();
    let old = side(parts.next()?, '-')?;
    let new = side(parts.next()?, '+')?;
    (parts.next()? == "@@").then_some((old, new))
}

fn lines(patch: &str, before: Option<&Source<'_>>, after: Option<&Source<'_>>) -> Vec<Line> {
    let (mut old, mut new, mut old_left, mut new_left) = (0, 0, 0, 0);
    patch
        .lines()
        .map(|line| {
            if let Some(((o, oc), (n, nc))) = hunk(line) {
                (old, new, old_left, new_left) = (o, n, oc, nc);
                return display(Kind::Hunk, line, None, None, Vec::new());
            }
            let (kind, code, old_number, new_number, source, number) = match line.as_bytes().first()
            {
                Some(b'-') if old_left > 0 => {
                    let number = old;
                    old += 1;
                    old_left -= 1;
                    (
                        Kind::Removed,
                        &line[1..],
                        Some(number),
                        None,
                        before,
                        number,
                    )
                }
                Some(b'+') if new_left > 0 => {
                    let number = new;
                    new += 1;
                    new_left -= 1;
                    (Kind::Added, &line[1..], None, Some(number), after, number)
                }
                Some(b' ') if old_left > 0 && new_left > 0 => {
                    let (o, n) = (old, new);
                    old += 1;
                    new += 1;
                    old_left -= 1;
                    new_left -= 1;
                    (Kind::Context, &line[1..], Some(o), Some(n), after, n)
                }
                _ => {
                    // Nonstandard hunks must not inherit standard hunk offsets.
                    if !line.starts_with("\\ No newline") {
                        old_left = 0;
                        new_left = 0;
                    }
                    return display(Kind::Meta, line, None, None, Vec::new());
                }
            };
            let spans = source
                .map(|source| source.spans(number, code))
                .unwrap_or_default();
            display(kind, code, old_number, new_number, spans)
        })
        .collect()
}

fn display(kind: Kind, raw: &str, old: Option<usize>, new: Option<usize>, spans: Spans) -> Line {
    let cut = raw
        .char_indices()
        .nth(LINE_LIMIT)
        .map_or(raw.len(), |(ix, _)| ix);
    let mut text = String::new();
    let mut positions = vec![0; cut + 1];
    for (ix, ch) in raw[..cut].char_indices() {
        positions[ix] = text.len();
        if ch == '\t' {
            text.push_str("    ");
        } else {
            text.push(ch);
        }
        positions[ix + ch.len_utf8()] = text.len();
    }
    let spans = spans
        .into_iter()
        .filter_map(|(range, kind)| {
            let end = range.end.min(cut);
            (range.start < end).then(|| (positions[range.start]..positions[end], kind))
        })
        .collect();
    if cut < raw.len() {
        text.push_str(" … [line truncated]");
    }
    Line {
        kind,
        old,
        new,
        text,
        spans,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/git_preview.rs"]
mod tests;
