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
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{PROJECT, fields, many, on_the_rail, root},
};
use artifact::{
    article::{self, properties},
    stamp,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ARTICLE: Arg = Arg {
    name: "article",
    about: "The article: its project reference (#12), title, or storage id.",
};
/// The same argument where several are taken at once. A second const rather
/// than a flag on [`ARTICLE`], the way `board::CARDS` stands beside
/// `board::CARD`: the line a client reads is the whole of how it learns a list
/// is allowed here.
const ARTICLES: Arg = Arg {
    name: "article",
    about: "The article, or several: each its project reference (#12), title, or storage id.",
};

/// `title` and `text` each carry one line when the article is written and
/// another when it is rewritten, so there is a const for each rather than one
/// wording made to cover both.
const TITLE: Arg = Arg {
    name: "title",
    about: "What the article is called.",
};
const TITLE_NOW: Arg = Arg {
    name: "title",
    about: "What it should be called now.",
};
const MARKDOWN: Arg = Arg {
    name: "text",
    about: "Its markdown.",
};
const MARKDOWN_NOW: Arg = Arg {
    name: "text",
    about: "The markdown it should hold now.",
};

/// Where a move puts it. Its own argument rather than [`PROJECT`], which is
/// the project the article is in now.
const TO_PROJECT: Arg = Arg {
    name: "to_project",
    about: "The project to move it to: the path of the directory, which must be one cydonia has open.",
};

const OLD_STRING: Arg = Arg {
    name: "old_string",
    about: "The exact text to replace, including whitespace. Include surrounding text to identify a unique occurrence. Must not be empty.",
};
const NEW_STRING: Arg = Arg {
    name: "new_string",
    about: "The replacement text. Use an empty string to delete the matched text.",
};
const REPLACE_ALL: Arg = Arg {
    name: "replace_all",
    about: "Replace every non-overlapping occurrence. Defaults to false, requiring exactly one match.",
};

pub static TOOLS: [Tool; 7] = [
    Tool {
        name: "article_list",
        description: "List the project's articles, most recently written first.",
        schema: |bound| fields(bound, &[PROJECT]),
        writes: false,
        call: list,
    },
    Tool {
        name: "article_read",
        description: "Read one article's markdown. The result includes assets_path, the shared media directory on the Cydonia host; filesystem access is needed to place images there.",
        schema: |bound| fields(bound, &[PROJECT, ARTICLE]),
        writes: false,
        call: read,
    },
    Tool {
        name: "article_add",
        description: "Write a new article, and answer its id and assets_path, the shared media directory on the Cydonia host. This tool writes Markdown, not image bytes.",
        schema: |bound| fields(bound, &[PROJECT, TITLE, MARKDOWN]),
        writes: true,
        call: add,
    },
    Tool {
        name: "article_rewrite",
        description: "Replace an article's markdown. The title is left alone.",
        schema: |bound| fields(bound, &[PROJECT, ARTICLE, MARKDOWN_NOW]),
        writes: true,
        call: rewrite,
    },
    Tool {
        name: "article_edit",
        description: "Edit an article's markdown by exact string replacement. Read it first. Missing or ambiguous matches leave it unchanged; include more context to target one occurrence, or set replace_all to change all. The title is left alone.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ARTICLE, OLD_STRING, NEW_STRING]);
            schema["properties"][REPLACE_ALL.name] = json!({
                "type": "boolean",
                "description": REPLACE_ALL.about,
                "default": false,
            });
            schema["properties"][OLD_STRING.name]["minLength"] = json!(1);
            schema
        },
        writes: true,
        call: edit,
    },
    Tool {
        name: "article_move",
        description: "Move an article to another project, or several in one write, each with its cover and the pictures in its body. Their project references (#12) change, since numbers are per project.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ARTICLES, TO_PROJECT]);
            many(&mut schema, ARTICLES);
            schema
        },
        writes: true,
        call: move_article,
    },
    Tool {
        name: "article_rename",
        description: "Rename an article. What it is filed under does not change.",
        schema: |bound| fields(bound, &[PROJECT, ARTICLE, TITLE_NOW]),
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
                "number": article.number,
                "title": article.title,
                "archived": article.archived,
                "touched": article.touched.to_string(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Answer::said(listing(&held)).with(json!({ "articles": data })))
}

fn read(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let assets = assets_path(project)?;
    let found = locate(project, args.text(ARTICLE)?)?;
    let text = std::fs::read_to_string(&found.content)
        .map_err(|e| Trouble::Refused(format!("{} cannot be read — {e}", found.label())))?;
    Ok(Answer::said(text)
        .with(json!({ "id": found.id, "number": found.number, "title": found.title, "assets_path": assets })))
}

fn add(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let title = args.text(TITLE)?;
    let text = args.text(MARKDOWN)?;
    let assets = assets_path(project)?;
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
    let number = artifact::entry::number(project, "article", &id)
        .map_err(|e| Trouble::Refused(e.to_string()))?;
    Ok(Answer::said(format!("#{number} {title} written"))
        .with(json!({ "id": id, "number": number, "title": title, "assets_path": assets })))
}

fn assets_path(project: &Path) -> Result<PathBuf, Trouble> {
    let root = std::fs::canonicalize(project)
        .map_err(|e| Trouble::Refused(format!("the project path cannot be resolved — {e}")))?;
    Ok(artifact::project::fs::Project::new(root).assets())
}

fn rewrite(args: Args<'_>) -> Outcome {
    let found = locate(root(&args)?, args.text(ARTICLE)?)?;
    let text = args.text(MARKDOWN_NOW)?;
    std::fs::write(&found.content, text)
        .map_err(|e| Trouble::Refused(format!("{} cannot be written — {e}", found.label())))?;
    Ok(Answer::said(format!("{} rewritten", found.label())))
}

fn edit(args: Args<'_>) -> Outcome {
    let old = args.text(OLD_STRING)?;
    let new = args.text(NEW_STRING)?;
    let all = args.boolean(REPLACE_ALL, false)?;
    if old.is_empty() {
        return Err(Trouble::Invalid("old_string must not be empty".to_owned()));
    }
    let found = locate(root(&args)?, args.text(ARTICLE)?)?;
    let text = std::fs::read_to_string(&found.content)
        .map_err(|e| Trouble::Refused(format!("{} cannot be read — {e}", found.label())))?;
    let count = text.matches(old).count();
    if count == 0 {
        return Err(Trouble::Refused(
            "old_string was not found; read the article and provide exact text, including whitespace"
                .to_owned(),
        ));
    }
    if count > 1 && !all {
        return Err(Trouble::Refused(format!(
            "old_string matches {count} occurrences; include more surrounding text for a unique match, or set replace_all to true"
        )));
    }
    let edited = text.replacen(old, new, count);
    std::fs::write(&found.content, edited)
        .map_err(|e| Trouble::Refused(format!("{} cannot be written — {e}", found.label())))?;
    Ok(
        Answer::said(format!("{} edited: {count} replacement(s)", found.label()))
            .with(json!({ "id": found.id, "replacements": count })),
    )
}

fn rename(args: Args<'_>) -> Outcome {
    let found = locate(root(&args)?, args.text(ARTICLE)?)?;
    let title = args.text(TITLE_NOW)?;
    properties::set_title(&found.content, title);
    Ok(Answer::said(format!("{} is now {title}", found.label())))
}

// ── addressing ───────────────────────────────────────────────────

/// One article as this surface reads it. Not `artifact::article::Article`,
/// which carries a cover this has no use for and no path, which is the whole of
/// what a write needs.
struct Held {
    number: Option<u64>,
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
/// One article or a run of them, into one project.
///
/// Every article is found before any is moved, so a title that names nothing
/// refuses the whole call rather than the half that was left. The move itself
/// is a file at a time and cannot be undone partway: a failure there says
/// which ones had already landed.
fn move_article(args: Args<'_>) -> Outcome {
    let from = root(&args)?;
    let found: Vec<Held> = args
        .list(ARTICLES)?
        .into_iter()
        .map(|needle| locate(from, needle))
        .collect::<Result<_, _>>()?;
    let to = on_the_rail(Path::new(args.text(TO_PROJECT)?))?;
    if to == from {
        return Err(Trouble::Refused(format!(
            "{} is already in {}",
            found
                .iter()
                .map(Held::label)
                .collect::<Vec<_>>()
                .join(", "),
            from.display()
        )));
    }
    let mut landed: Vec<Value> = Vec::new();
    let mut spoken: Vec<String> = Vec::new();
    for held in &found {
        let label = held.label().to_owned();
        let arrived = article::move_to(&held.content, to).map_err(|e| {
            Trouble::Refused(match spoken.is_empty() {
                true => format!("{label} cannot be moved — {e}"),
                false => format!(
                    "{label} cannot be moved — {e}. {} had already landed",
                    spoken.join(", ")
                ),
            })
        })?;
        let id = article::id_of(&arrived);
        let number = artifact::entry::number(to, "article", &id)
            .map_err(|e| Trouble::Refused(e.to_string()))?;
        spoken.push(format!("{label} as #{number}"));
        landed.push(json!({
            "id": id,
            "number": number,
            "title": held.title,
        }));
    }
    Ok(
        Answer::said(format!("{} moved to {}", spoken.join(", "), to.display())).with(json!({
            "articles": landed,
            "project": to,
            "assets_path": assets_path(to)?,
        })),
    )
}

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
                number: artifact::entry::number(project, "article", &article::id_of(&content)).ok(),
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
    if let Some(number) = artifact::entry::reference(needle) {
        return held
            .iter()
            .position(|article| article.number == Some(number))
            .map(|at| held.swap_remove(at))
            .ok_or_else(|| Trouble::Refused(format!("no article {needle} in this project")));
    }
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
    held.iter()
        .map(|article| {
            let archived = match article.archived {
                true => "  — archived",
                false => "",
            };
            format!(
                "{}  {}{archived}",
                artifact::entry::label(article.number, article.label()),
                article.id
            )
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
