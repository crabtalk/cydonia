//! Which language a file is written in, and whether this build can colour it.
//!
//! TODO: [`KNOWN`] duplicates the identity half of `syntax::lang::LANGS` and is
//! maintained by hand against it, across a crate boundary. It exists because
//! that table holds no entry for a language it has no grammar for.

use std::{ops::Range, path::Path};

use bezel::theme::HighlightKind;

/// File names and extensions, per language. Names first in each row, because
/// `Dockerfile` is a whole name rather than a suffix; the rest are matched as
/// extensions. Ids are the fence tags `syntax::lang` answers to where a
/// grammar exists, so the same string decides both questions.
const KNOWN: &[(&str, &[&str])] = &[
    // Carried by this build.
    (
        "bash",
        &[
            ".bash_profile",
            ".bashrc",
            ".profile",
            ".zshrc",
            "bash",
            "sh",
            "zsh",
        ],
    ),
    ("go", &["go"]),
    ("json", &["json", "jsonc"]),
    ("markdown", &["markdown", "md"]),
    ("python", &["py", "pyi"]),
    ("rust", &["rs"]),
    ("toml", &["toml"]),
    ("tsx", &["cjs", "jsx", "mjs", "tsx"]),
    ("typescript", &["cts", "mts", "ts"]),
    // Named, but with no grammar here.
    ("c", &["c", "h"]),
    ("cpp", &["cc", "cpp", "cxx", "hpp"]),
    ("csharp", &["cs"]),
    ("css", &["css", "scss"]),
    ("dockerfile", &["Containerfile", "Dockerfile"]),
    ("elixir", &["ex", "exs"]),
    ("graphql", &["gql", "graphql"]),
    ("haskell", &["hs"]),
    ("html", &["htm", "html"]),
    ("java", &["java"]),
    ("kotlin", &["kt", "kts"]),
    ("lua", &["lua"]),
    ("make", &["Makefile", "mk"]),
    ("nix", &["nix"]),
    ("php", &["php"]),
    ("proto", &["proto"]),
    ("ruby", &["Gemfile", "Rakefile", "erb", "rb"]),
    ("scala", &["sbt", "scala"]),
    ("sql", &["sql"]),
    ("svelte", &["svelte"]),
    ("swift", &["swift"]),
    ("vue", &["vue"]),
    ("xml", &["xml"]),
    ("yaml", &["yaml", "yml"]),
    ("zig", &["zig"]),
];

/// What a file's name says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// A grammar this build carries, under the name `syntax` knows it by.
    Ready(&'static str),
    /// Markdown, which bezel highlights without a tree-sitter grammar.
    Markdown,
    /// A language this build can name and cannot paint.
    Missing(&'static str),
}

/// The language `path` is written in, or `None` when its name names nothing
/// this table carries.
///
/// The whole name wins over an extension — `Dockerfile` is not a `.file` — and
/// between extensions the longest match wins, so a future `.d.ts` beats `.ts`
/// rather than racing it.
pub fn of(path: &Path) -> Option<Language> {
    let name = path.file_name()?.to_str()?;
    let id = KNOWN
        .iter()
        .filter_map(|(id, names)| {
            names
                .iter()
                .filter(|candidate| {
                    name.eq_ignore_ascii_case(candidate)
                        || name.len() > candidate.len()
                            && name[name.len() - candidate.len() - 1..]
                                .eq_ignore_ascii_case(&format!(".{candidate}"))
                })
                .map(|candidate| (candidate.len(), *id))
                .max()
        })
        .max()
        .map(|(_, id)| id)?;
    Some(match id {
        "markdown" => Language::Markdown,
        id => match syntax::lang::resolve(id) {
            Some(language) => Language::Ready(language.name),
            None => Language::Missing(id),
        },
    })
}

/// The spans `text` is painted with as the contents of `path`, or `None` where
/// nothing here can paint it.
///
/// The single place that answers how a file is coloured; the file view and the
/// diff preview both route through here.
pub fn spans(path: &Path, text: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
    match of(path)? {
        Language::Ready(language) => syntax::highlight(text, language),
        Language::Markdown => Some(markdown::source::spans(text)),
        Language::Missing(_) => None,
    }
}
