//! The tools an agent works a project's articles through.
//!
//! An article is a directory holding `content.md` and the properties beside
//! it, so what these move is two files and the layout is `artifact::article`'s
//! to know. Nothing here opens a document in anything — what an article is
//! opened *in* is the app's, and stays there.
//!
//! Addressed by title, which is what a person says, and by id, which is what a
//! caller keeps. An article that has never been named has only the second, and
//! is listed under it.
//!
//! Putting one away is not here. Archiving is a judgement about your own
//! workspace, and an agent that wanted it wanted a person to make it.

use crate::{
    tool::{Answer, Args, Outcome, Tool, Trouble},
    tools::{PROJECT, fields, root},
};
use artifact::{
    article::{self, properties},
    stamp,
};
use serde_json::json;
use std::path::{Path, PathBuf};

const ARTICLE: &str = "The article: its title, or its id.";

pub static TOOLS: [Tool; 5] = [
    Tool {
        name: "article_list",
        description: "List the project's articles, most recently written first.",
        schema: |bound| fields(bound, &[("project", PROJECT)]),
        writes: false,
        call: list,
    },
    Tool {
        name: "article_read",
        description: "Read one article's markdown.",
        schema: |bound| fields(bound, &[("project", PROJECT), ("article", ARTICLE)]),
        writes: false,
        call: read,
    },
    Tool {
        name: "article_add",
        description: "Write a new article, and answer the id it is filed under.",
        schema: |bound| {
            fields(
                bound,
                &[
                    ("project", PROJECT),
                    ("title", "What the article is called."),
                    ("text", "Its markdown."),
                ],
            )
        },
        writes: true,
        call: add,
    },
    Tool {
        name: "article_rewrite",
        description: "Replace an article's markdown. The title is left alone.",
        schema: |bound| {
            fields(
                bound,
                &[
                    ("project", PROJECT),
                    ("article", ARTICLE),
                    ("text", "The markdown it should hold now."),
                ],
            )
        },
        writes: true,
        call: rewrite,
    },
    Tool {
        name: "article_rename",
        description: "Rename an article. What it is filed under does not change.",
        schema: |bound| {
            fields(
                bound,
                &[
                    ("project", PROJECT),
                    ("article", ARTICLE),
                    ("title", "What it should be called now."),
                ],
            )
        },
        writes: true,
        call: rename,
    },
];

// ── the tools ────────────────────────────────────────────────────

fn list(args: Args<'_>) -> Outcome {
    let held = articles(root(&args)?);
    if held.is_empty() {
        return Ok(Answer::said("this project has no articles"));
    }
    let data = held
        .iter()
        .map(|article| {
            json!({
                "id": article.id,
                "title": article.title,
                "archived": article.archived,
                "touched": article.touched.to_string(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Answer::said(listing(&held)).with(json!({ "articles": data })))
}

fn read(args: Args<'_>) -> Outcome {
    let found = locate(root(&args)?, args.text("article")?)?;
    let text = std::fs::read_to_string(&found.content)
        .map_err(|e| Trouble::Refused(format!("{} cannot be read — {e}", found.label())))?;
    Ok(Answer::said(text).with(json!({ "id": found.id, "title": found.title })))
}

fn add(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let title = args.text("title")?;
    let text = args.text("text")?;
    let dir = article::init(project).map_err(|e| {
        Trouble::Refused(format!("{} cannot be written to — {e}", project.display()))
    })?;
    let home = article::free(&dir, stamp::now());
    let content = article::content(&home);
    std::fs::create_dir_all(&home)
        .and_then(|()| std::fs::write(&content, text))
        .map_err(|e| Trouble::Refused(format!("the article cannot be written — {e}")))?;
    // After the document, because the properties file sits beside it and the
    // directory has to be there first.
    properties::set_title(&content, title);
    let id = article::id_of(&content);
    Ok(Answer::said(format!("{title} written")).with(json!({ "id": id, "title": title })))
}

fn rewrite(args: Args<'_>) -> Outcome {
    let found = locate(root(&args)?, args.text("article")?)?;
    let text = args.text("text")?;
    std::fs::write(&found.content, text)
        .map_err(|e| Trouble::Refused(format!("{} cannot be written — {e}", found.label())))?;
    Ok(Answer::said(format!("{} rewritten", found.label())))
}

fn rename(args: Args<'_>) -> Outcome {
    let found = locate(root(&args)?, args.text("article")?)?;
    let title = args.text("title")?;
    properties::set_title(&found.content, title);
    Ok(Answer::said(format!("{} is now {title}", found.label())))
}

// ── addressing ───────────────────────────────────────────────────

/// One article as this surface reads it. Not `artifact::article::Article`,
/// which carries a cover this has no use for and no path, which is the whole of
/// what a write needs.
struct Held {
    id: String,
    title: String,
    archived: bool,
    touched: u128,
    content: PathBuf,
}

impl Held {
    /// What to call it: its title, or its id for one that has never been named.
    fn label(&self) -> &str {
        match self.title.is_empty() {
            true => &self.id,
            false => &self.title,
        }
    }
}

/// Every article in a project, most recently written first.
///
/// A project written by an older cydonia may still hold loose `.md` files that
/// the app moves into directories when it opens the project. Those are not
/// migrated here: a read reaching in from a port has no business rearranging
/// somebody's files, and the app will have done it by the time an agent is
/// running in there.
fn articles(project: &Path) -> Vec<Held> {
    let Ok(entries) = std::fs::read_dir(article::dir(project)) else {
        return Vec::new();
    };
    let mut held: Vec<Held> = entries
        .flatten()
        .map(|entry| article::content(&entry.path()))
        .filter(|content| content.is_file())
        .map(|content| {
            let held = properties::all(&content);
            Held {
                id: article::id_of(&content),
                title: held.title,
                archived: held.archived,
                touched: article::touched(&content),
                content,
            }
        })
        .collect();
    held.sort_by_key(|article| std::cmp::Reverse(article.touched));
    held
}

/// The article a needle names: its id, or its title. A title that hits twice is
/// refused rather than guessed at — nothing stops two articles sharing one, and
/// the caller is one `list_articles` away from the ids.
fn locate(project: &Path, needle: &str) -> Result<Held, Trouble> {
    let mut held = articles(project);
    if let Some(at) = held.iter().position(|article| article.id == needle) {
        return Ok(held.swap_remove(at));
    }
    let titled: Vec<usize> = held
        .iter()
        .enumerate()
        .filter(|(_, article)| article.title.eq_ignore_ascii_case(needle))
        .map(|(at, _)| at)
        .collect();
    match titled.as_slice() {
        [at] => Ok(held.swap_remove(*at)),
        [] => Err(Trouble::Refused(format!(
            "no article {needle} — this project has {}",
            titles(&held)
        ))),
        _ => Err(Trouble::Refused(format!(
            "two articles are called {needle} — name the one you mean by its id"
        ))),
    }
}

// ── rendering ────────────────────────────────────────────────────

/// Every article, one to a line: what it is called, and what to ask for it by
/// when two share a name.
fn listing(held: &[Held]) -> String {
    let width = held
        .iter()
        .map(|article| article.label().chars().count())
        .max()
        .unwrap_or(0);
    held.iter()
        .map(|article| {
            let archived = match article.archived {
                true => "  — archived",
                false => "",
            };
            format!("{:<width$}  {}{archived}", article.label(), article.id)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn titles(held: &[Held]) -> String {
    match held.is_empty() {
        true => "none at all".to_owned(),
        false => held
            .iter()
            .map(|article| article.label().to_owned())
            .collect::<Vec<_>>()
            .join(", "),
    }
}
