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
    tools::{HITS, PROJECT, Project, QUERY, fields, many, on_the_rail, root, session},
};
use artifact::error::{Act, Error};
use artifact::{
    article::{
        self, anchor,
        comments::{Author, Comment, Reply},
        properties::Properties,
    },
    kind::Kind,
    location::Location,
    project::Project as _,
    reference::{Span, Within},
    search::{Block, Query},
    stamp,
};
use futures::executor::block_on;
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::mpsc};

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

/// The picture to stand over the article. A path on the Cydonia host, relative
/// to the project's directory, so a client without filesystem access there has
/// nothing to name — see
/// [`artifact::article::cover`] for where it lands.
const COVER: Arg = Arg {
    name: "image",
    about: "The picture, as a path relative to the project directory, such as one inside the article's assets_path. Draw or crop it 5:2 — 1500x600 is the size the app cuts its own at, and a picture of another shape is not cropped to fit. Left out, the cover is taken off.",
};

/// Which way the switch goes. Defaulted to putting away, because that is what
/// a caller reaching for this almost always means — and the other way is
/// spelled out rather than left to a bare call.
const ARCHIVED: Arg = Arg {
    name: "archived",
    about: "true to put it away, false to bring it back. Left out, it is put away.",
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
    about: "The project to move it to: the path of the directory, or the cupola:// location cydonia lists for one on another machine, which must be one cydonia has open.",
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

const QUOTE: Arg = Arg {
    name: "quote",
    about: "The exact text of the article's markdown the note is on, including whitespace. It must occur once; include surrounding text to tell occurrences apart. Must not be empty.",
};
const NOTE: Arg = Arg {
    name: "note",
    about: "The note, in Cydonia Markdown. Shown in the margin beside the quote, a few lines of it until it is opened.",
};

const COMMENT: Arg = Arg {
    name: "comment",
    about: "The note's id, as the follow-up names it.",
};
const ANSWER: Arg = Arg {
    name: "answer",
    about: "The answer, in Cydonia Markdown. Shown in the note's thread under the follow-up it answers.",
};

pub static TOOLS: [Tool; 14] = [
    Tool {
        name: "article_list",
        description: "List the project's articles, most recently written first.",
        schema: |bound| fields(bound, &[PROJECT]),
        writes: false,
        deletes: false,
        call: list,
    },
    Tool {
        name: "article_read",
        description: "Read one article's markdown, or part of it: #12:5-7 reads lines 5 to 7, and #12#setup the section under the heading whose anchor is setup (GitHub's: lowercased, punctuation dropped, spaces as -). A part answers its line numbers. sessions are the numbers of the sessions opened under the article, newest first; read one with session_read. The result includes two directories, relative to the project directory: assets_path, this article's own media directory, which holds its body images and its cover; and article_path, the article's folder. Filesystem access is needed to place a file in either.",
        schema: |bound| fields(bound, &[PROJECT, ARTICLE]),
        writes: false,
        deletes: false,
        call: read,
    },
    Tool {
        name: "article_search",
        description: "Find text in the project's articles, archived ones included: their titles and their markdown. Answers each match with the article's reference (#12), title, line number and the line it matched.",
        schema: |bound| fields(bound, &[PROJECT, QUERY]),
        writes: false,
        deletes: false,
        call: search,
    },
    Tool {
        name: "article_highlights",
        description: "List the passages a person highlighted in one article (`==text==` in its markdown), in document order, each with its line number and the paragraph it sits in.",
        schema: |bound| fields(bound, &[PROJECT, ARTICLE]),
        writes: false,
        deletes: false,
        call: highlights,
    },
    Tool {
        name: "article_add",
        description: "Write a new article, and answer its id, assets_path, the article's own media directory where body images and its cover go, and article_path, the article's folder, both relative to the project directory. This tool writes Markdown, not image bytes; a cover is set with article_set_cover rather than written into assets_path.",
        schema: |bound| fields(bound, &[PROJECT, TITLE, MARKDOWN]),
        writes: true,
        deletes: false,
        call: add,
    },
    Tool {
        name: "article_rewrite",
        description: "Replace an article's markdown. The title is left alone.",
        schema: |bound| fields(bound, &[PROJECT, ARTICLE, MARKDOWN_NOW]),
        writes: true,
        deletes: false,
        call: rewrite,
    },
    Tool {
        name: "article_edit",
        description: "Edit an article's markdown by exact string replacement. Read it first. Missing or ambiguous matches leave it unchanged; include more context to target one occurrence, or set replace_all to change all. The title is left alone.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ARTICLE, OLD_STRING, NEW_STRING]);
            super::defaulted(&mut schema, REPLACE_ALL, "boolean", json!(false));
            schema["properties"][OLD_STRING.name]["minLength"] = json!(1);
            schema
        },
        writes: true,
        deletes: false,
        call: edit,
    },
    Tool {
        name: "article_comment",
        description: "File a note in an article's margin, against a passage of its markdown. The article itself is not changed. The note links back to the turn of the session that filed it.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ARTICLE, QUOTE, NOTE]);
            schema["properties"][QUOTE.name]["minLength"] = json!(1);
            schema
        },
        writes: true,
        deletes: false,
        call: comment,
    },
    Tool {
        name: "article_comment_reply",
        description: "Answer a follow-up asked on a note in an article's margin. The answer joins the note's thread; the article itself is not changed.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ARTICLE, ANSWER]);
            schema["properties"][COMMENT.name] =
                json!({ "type": "integer", "description": COMMENT.about });
            if let Some(required) = schema["required"].as_array_mut() {
                required.push(json!(COMMENT.name));
            }
            schema
        },
        writes: true,
        deletes: false,
        call: comment_reply,
    },
    Tool {
        name: "article_set_cover",
        description: "Put a picture over an article, or take the one it has off. The file is filed under the article's own folder, which is not where body images go. Draw it 5:2 (1500x600): it is shown at that shape and not cropped to it.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ARTICLE]);
            schema["properties"][COVER.name] = json!({
                "type": "string",
                "description": COVER.about,
            });
            schema
        },
        writes: true,
        deletes: false,
        call: set_cover,
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
        deletes: false,
        call: move_article,
    },
    Tool {
        name: "article_archive",
        description: "Put an article away, or bring one back. An archived article is listed under the divider rather than gone, and is still read and written by every other tool.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ARTICLES]);
            many(&mut schema, ARTICLES);
            super::defaulted(&mut schema, ARCHIVED, "boolean", json!(true));
            schema
        },
        writes: true,
        deletes: false,
        call: archive,
    },
    Tool {
        name: "article_remove",
        description: "Delete an article and everything filed with it — its cover, its pictures and its number. This cannot be undone; archive it instead to put it away.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, ARTICLES]);
            many(&mut schema, ARTICLES);
            schema
        },
        writes: true,
        deletes: true,
        call: remove,
    },
    Tool {
        name: "article_rename",
        description: "Rename an article. What it is filed under does not change.",
        schema: |bound| fields(bound, &[PROJECT, ARTICLE, TITLE_NOW]),
        writes: true,
        deletes: false,
        call: rename,
    },
];

// ── the tools ────────────────────────────────────────────────────

fn list(args: Args<'_>) -> Outcome {
    let held = articles(&root(&args)?);
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

fn search(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let query = Query::literal(args.text(QUERY)?)
        .ok_or_else(|| Trouble::Refused("query is empty".to_owned()))?;
    let (tx, rx) = mpsc::channel();
    project.store.search(&[Kind::Article], &query, &tx);
    drop(tx);
    let held: HashMap<String, Held> = articles(&project)
        .into_iter()
        .map(|article| (article.id.clone(), article))
        .collect();
    // The title first, then the lines in order: one hit per line.
    let mut found: Vec<(u64, usize, &Held, String)> = rx
        .into_iter()
        .filter_map(|hit| {
            let article = held.get(&hit.item.id)?;
            let line = match hit.block {
                Block::Title => 0,
                Block::Line(ix) => ix + 1,
                _ => return None,
            };
            Some((article.number.unwrap_or(u64::MAX), line, article, hit.line))
        })
        .collect();
    found.sort_by_key(|(number, line, ..)| (*number, *line));
    found.dedup_by_key(|(number, line, ..)| (*number, *line));
    if found.is_empty() {
        return Ok(
            Answer::said(format!("nothing matches {}", query.text())).with(json!({ "hits": [] }))
        );
    }
    let more = found.len() > HITS;
    found.truncate(HITS);
    let hits: Vec<Value> = found
        .iter()
        .map(|(_, line, article, text)| {
            json!({
                "article": artifact::entry::label(article.number, article.label()),
                "id": article.id,
                "line": (*line > 0).then_some(*line),
                "text": session::snippet(text),
            })
        })
        .collect();
    let mut text = found
        .iter()
        .map(|(_, line, article, text)| {
            let at = match line {
                0 => "title".to_owned(),
                line => format!("line {line}"),
            };
            format!(
                "{} {at} — {}",
                artifact::entry::label(article.number, article.label()),
                session::snippet(text)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if more {
        text.push_str(&format!(
            "\n… more than {HITS} lines match; narrow the query"
        ));
    }
    Ok(Answer::said(text).with(json!({ "hits": hits, "more": more })))
}

fn read(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let named = args.text(ARTICLE)?;
    let (found, part) = located(&project, named)?;
    let text = project
        .store
        .read_article(&found.id)
        .map_err(|e| Error::failed(found.label(), Act::Read, e))?;
    let (text, lines) = match part {
        None => (text, None),
        Some(part) => {
            let (range, lines) = excerpt(&text, part, named)?;
            (text[range].to_owned(), Some(lines))
        }
    };
    let mut opened: Vec<_> = project
        .store
        .sessions()
        .into_iter()
        .filter(|record| record.article.as_deref() == Some(found.id.as_str()))
        .collect();
    opened.sort_by_key(|record| std::cmp::Reverse(record.updated));
    let sessions: Vec<_> = opened.iter().filter_map(|record| record.number).collect();
    Ok(Answer::said(text).with(json!({
        "id": found.id,
        "number": found.number,
        "title": found.title,
        "sessions": sessions,
        "lines": lines.map(|lines| json!({ "from": lines.from, "to": lines.to })),
        "assets_path": found.dir.as_deref().map(|dir| project.within(&article::assets(&article::content(dir)))),
        "article_path": found.dir.as_deref().map(|dir| project.within(dir)),
        "cover_path": found.cover.as_deref().map(|cover| project.within(cover)),
    })))
}

fn highlights(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let found = locate(&project, args.text(ARTICLE)?)?;
    let text = project
        .store
        .read_article(&found.id)
        .map_err(|e| Error::failed(found.label(), Act::Read, e))?;
    let found_marks = marked(&text);
    if found_marks.is_empty() {
        return Ok(Answer::said(format!("{} has no highlights", found.label())));
    }
    let said = found_marks
        .iter()
        .map(|mark| format!("line {}: {}", mark.line, mark.text.replace('\n', " ")))
        .collect::<Vec<_>>()
        .join("\n");
    let data = found_marks
        .iter()
        .map(|mark| json!({ "text": mark.text, "line": mark.line, "paragraph": mark.paragraph }))
        .collect::<Vec<_>>();
    Ok(Answer::said(said).with(json!({
        "id": found.id,
        "number": found.number,
        "title": found.title,
        "highlights": data,
    })))
}

/// One `==text==` span.
struct Marked {
    text: String,
    /// 1-based line the span opens on.
    line: usize,
    paragraph: String,
}

/// Every `==text==` span in `markdown`, outside fences and inline code. A span
/// never crosses a blank line.
fn marked(markdown: &str) -> Vec<Marked> {
    let mut found = Vec::new();
    let mut fence: Option<&str> = None;
    let mut paragraph: Vec<(usize, &str)> = Vec::new();
    for (ix, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        let opener = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, opener) {
            (Some(open), Some(marker)) if open == marker => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, Some(marker)) => {
                scan(&paragraph, &mut found);
                paragraph.clear();
                fence = Some(marker);
                continue;
            }
            (None, None) => {}
        }
        if trimmed.is_empty() {
            scan(&paragraph, &mut found);
            paragraph.clear();
        } else {
            paragraph.push((ix + 1, line));
        }
    }
    scan(&paragraph, &mut found);
    found
}

fn scan(lines: &[(usize, &str)], found: &mut Vec<Marked>) {
    let Some(&(first, _)) = lines.first() else {
        return;
    };
    let text = lines
        .iter()
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut open: Option<usize> = None;
    while at < bytes.len() {
        if bytes[at] == b'`' {
            let run = bytes[at..].iter().take_while(|&&b| b == b'`').count();
            let fence = &text[at..at + run];
            match text[at + run..].find(fence) {
                Some(close) => at += run + close + run,
                None => at += run,
            }
            continue;
        }
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if bytes[at..].starts_with(b"==") {
            match open {
                Some(start) if at > start && !text[start..at].trim().is_empty() => {
                    found.push(Marked {
                        text: text[start..at].to_string(),
                        line: first + text[..start].matches('\n').count(),
                        paragraph: text.clone(),
                    });
                    open = None;
                }
                Some(_) => open = None,
                None => open = Some(at + 2),
            }
            at += 2;
            continue;
        }
        at += 1;
    }
}

fn add(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let title = args.text(TITLE)?;
    let text = args.text(MARKDOWN)?;
    let store = &project.store;
    let made = block_on(store.create_article(text))
        .map_err(|e| Error::failed("the article", Act::Written, e))?;
    let titled = Properties {
        title: title.to_owned(),
        ..Properties::default()
    };
    block_on(store.save_properties(&made.id, &titled))
        .map_err(|e| Error::failed("the article", Act::Named, e))?;
    let id = made.id;
    let dir = store.place(Kind::Article, &id);
    let number =
        block_on(store.number(Kind::Article, &id)).map_err(|e| Trouble::Refused(e.to_string()))?;
    Ok(
        Answer::said(format!("#{number} {title} written")).with(json!({
            "id": id,
            "number": number,
            "title": title,
            "assets_path": dir.as_deref().map(|dir| project.within(&article::assets(&article::content(dir)))),
            "article_path": dir.as_deref().map(|dir| project.within(dir)),
        })),
    )
}

/// File a picture as the article's cover, or take the one it has off.
///
/// The picture is imported rather than moved — see
/// [`article::cover::import`]: the source is the caller's, and a picture in the
/// article's `assets/` is one it may well link to as well.
/// What was there before goes, which is what keeps one article to one cover —
/// `cover::of` reads the first one it finds and a second would shadow it.
fn set_cover(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let found = locate(&project, args.text(ARTICLE)?)?;
    let files = project.files().ok_or_else(|| project.not_here("A cover"))?;
    let content = files
        .article_file(&found.id)
        .map_err(|e| Trouble::Refused(e.to_string()))?;
    let previous = article::cover::of(&content);
    let Some(source) = args.maybe(COVER) else {
        if let Some(old) = previous {
            std::fs::remove_file(&old).map_err(|e| {
                Error::failed(format!("{}'s cover", found.label()), Act::Deleted, e)
            })?;
        }
        return Ok(Answer::said(format!("{} has no cover now", found.label())));
    };
    let source = project.root().join(source);
    if !source.is_file() {
        return Err(Trouble::Refused(format!(
            "no picture at {} on this host",
            project.within(&source).display()
        )));
    }
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            Trouble::Refused(format!(
                "{} has no extension to name its format by",
                project.within(&source).display()
            ))
        })?;
    // Named for when it was filed, so a second cover never lands on the name
    // the first is cached under.
    let to = article::cover::path(&content, stamp::now(), &ext);
    if article::cover::is_cover(&source) && source == to {
        return Ok(Answer::said(format!("{} keeps its cover", found.label())));
    }
    article::cover::import(&source, &to)
        .map_err(|e| Error::failed("the cover", Act::Written, e))?;
    if let Some(old) = previous.filter(|old| *old != to) {
        let _ = std::fs::remove_file(old);
    }
    Ok(
        Answer::said(format!("{} is covered", found.label())).with(json!({
            "cover_path": project.within(&to),
            "article_path": content.parent().map(|dir| project.within(dir)),
        })),
    )
}

fn rewrite(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let found = locate(&project, args.text(ARTICLE)?)?;
    let text = args.text(MARKDOWN_NOW)?;
    block_on(project.store.write_article(&found.id, text))
        .map_err(|e| Error::failed(found.label(), Act::Written, e))?;
    Ok(Answer::said(format!("{} rewritten", found.label())))
}

fn edit(args: Args<'_>) -> Outcome {
    let old = args.text(OLD_STRING)?;
    let new = args.text(NEW_STRING)?;
    let all = args.boolean(REPLACE_ALL, false)?;
    if old.is_empty() {
        return Err(Trouble::Invalid("old_string must not be empty".to_owned()));
    }
    let project = root(&args)?;
    let found = locate(&project, args.text(ARTICLE)?)?;
    let text = project
        .store
        .read_article(&found.id)
        .map_err(|e| Error::failed(found.label(), Act::Read, e))?;
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
    block_on(project.store.write_article(&found.id, &edited))
        .map_err(|e| Error::failed(found.label(), Act::Written, e))?;
    Ok(
        Answer::said(format!("{} edited: {count} replacement(s)", found.label()))
            .with(json!({ "id": found.id, "replacements": count })),
    )
}

fn comment(args: Args<'_>) -> Outcome {
    let quote = args.text(QUOTE)?;
    let note = args.text(NOTE)?;
    if quote.is_empty() {
        return Err(Trouble::Invalid("quote must not be empty".to_owned()));
    }
    let project = root(&args)?;
    let found = locate(&project, args.text(ARTICLE)?)?;
    let store = &project.store;
    let text = store
        .read_article(&found.id)
        .map_err(|e| Error::failed(found.label(), Act::Read, e))?;
    let start = match text
        .match_indices(quote)
        .map(|(at, _)| at)
        .collect::<Vec<_>>()[..]
    {
        [at] => at,
        [] => {
            return Err(Trouble::Refused(
                "quote was not found; read the article and provide exact text, including whitespace"
                    .to_owned(),
            ));
        }
        ref many => {
            return Err(Trouble::Refused(format!(
                "quote matches {} occurrences; include more surrounding text for a unique match",
                many.len()
            )));
        }
    };
    let mut held = store.comments(&found.id);
    held.carry(None, &text);
    let id = held.fresh_id(stamp::now() as u64);
    held.comments.push(Comment::new(
        id,
        &text,
        start..start + quote.len(),
        note.to_owned(),
        session::turn_link(&args),
    ));
    block_on(store.save_comments(&found.id, &held))
        .map_err(|e| Error::failed(found.label(), Act::Written, e))?;
    Ok(Answer::said(format!("{} has a new comment", found.label()))
        .with(json!({ "id": found.id, "comment": id })))
}

fn comment_reply(args: Args<'_>) -> Outcome {
    let answer = args.text(ANSWER)?;
    let id = match args.value(COMMENT) {
        Some(Value::Number(id)) => id.as_u64(),
        Some(Value::String(id)) => id.trim().parse().ok(),
        _ => None,
    }
    .ok_or_else(|| Trouble::Invalid("comment is required, as the note's id".to_owned()))?;
    let project = root(&args)?;
    let found = locate(&project, args.text(ARTICLE)?)?;
    let store = &project.store;
    let mut held = store.comments(&found.id);
    let Some(comment) = held.get_mut(id) else {
        return Err(Trouble::Refused(format!(
            "{} has no note {id}",
            found.label()
        )));
    };
    comment.replies.push(Reply {
        from: Author::Agent,
        text: answer.to_owned(),
        turn: session::turn_link(&args),
    });
    block_on(store.save_comments(&found.id, &held))
        .map_err(|e| Error::failed(found.label(), Act::Written, e))?;
    Ok(
        Answer::said(format!("{} has an answer on note {id}", found.label()))
            .with(json!({ "id": found.id, "comment": id })),
    )
}

fn rename(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let found = locate(&project, args.text(ARTICLE)?)?;
    let title = args.text(TITLE_NOW)?;
    let store = &project.store;
    let mut held = store.properties(&found.id);
    held.title = title.to_owned();
    block_on(store.save_properties(&found.id, &held))
        .map_err(|e| Error::failed(found.label(), Act::Renamed, e))?;
    Ok(Answer::said(format!("{} is now {title}", found.label())))
}

/// One article or a run of them, put away or brought back in one write.
fn archive(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let archived = args.boolean(ARCHIVED, true)?;
    let mut said: Vec<String> = Vec::new();
    for needle in args.list(ARTICLES)? {
        let found = locate(&project, needle)?;
        let store = &project.store;
        let mut held = store.properties(&found.id);
        held.archived = archived;
        block_on(store.save_properties(&found.id, &held))
            .map_err(|e| Error::failed(found.label(), Act::Written, e))?;
        said.push(found.label().to_owned());
    }
    let what = match archived {
        true => "put away",
        false => "brought back",
    };
    Ok(Answer::said(format!("{} {what}", said.join(", "))))
}

/// One article or a run of them, off the disk for good.
///
/// Every one is found before any is removed: a run half deleted is not what a
/// refused call should leave behind, and a name that does not answer is the
/// usual reason one is refused.
fn remove(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let mut found = Vec::new();
    for needle in args.list(ARTICLES)? {
        found.push(locate(&project, needle)?);
    }
    let mut gone: Vec<String> = Vec::new();
    for article in &found {
        block_on(project.store.remove_article(&article.id))
            .map_err(|e| Error::failed(article.label(), Act::Deleted, e))?;
        gone.push(article.label().to_owned());
    }
    Ok(Answer::said(format!("{} deleted", gone.join(", "))))
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
    /// Its directory, on the project's machine.
    dir: Option<PathBuf>,
    /// Its cover, on the project's machine.
    cover: Option<PathBuf>,
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
        .map(|needle| locate(&from, needle))
        .collect::<Result<_, _>>()?;
    let to = Project::at(on_the_rail(Location::parse(args.text(TO_PROJECT)?))?)?;
    if to.location == from.location {
        return Err(Trouble::Refused(format!(
            "{} is already in {}",
            found.iter().map(Held::label).collect::<Vec<_>>().join(", "),
            from.location
        )));
    }
    let (Some(files), Some(_)) = (from.files(), to.files()) else {
        return Err(from.not_here("Moving an article to another project"));
    };
    let mut landed: Vec<Value> = Vec::new();
    let mut spoken: Vec<String> = Vec::new();
    for held in &found {
        let label = held.label().to_owned();
        let content = files
            .article_file(&held.id)
            .map_err(|e| Error::failed(label.clone(), Act::Moved, e))?;
        let arrived = article::move_to(&content, to.root()).map_err(|e| {
            let error = Error::failed(label.clone(), Act::Moved, e);
            Trouble::Refused(match spoken.is_empty() {
                true => error.to_string(),
                false => format!("{error}\n{} had already landed", spoken.join(", ")),
            })
        })?;
        let id = article::id_of(&arrived);
        let number = block_on(to.store.number(Kind::Article, &id))
            .map_err(|e| Trouble::Refused(e.to_string()))?;
        spoken.push(format!("{label} as #{number}"));
        landed.push(json!({
            "id": id,
            "number": number,
            "title": held.title,
            "assets_path": to.within(&article::assets(&arrived)),
            "article_path": arrived.parent().map(|dir| to.within(dir)),
        }));
    }
    Ok(
        Answer::said(format!("{} moved to {}", spoken.join(", "), to.location)).with(json!({
            "articles": landed,
            "project": to.location,
        })),
    )
}

fn articles(project: &Project) -> Vec<Held> {
    let store = &project.store;
    store
        .articles()
        .into_iter()
        .map(|found| Held {
            number: found
                .number
                .or_else(|| block_on(store.number(Kind::Article, &found.id)).ok()),
            dir: store.place(Kind::Article, &found.id),
            cover: found.cover.and_then(|cover| cover.to_file_path().ok()),
            id: found.id,
            title: found.title,
            archived: found.archived,
            touched: found.touched,
        })
        .collect()
}

/// The article a needle names: its id, or its title. A title that hits twice is
/// refused rather than guessed at — nothing stops two articles sharing one, and
/// the caller is one `list_articles` away from the ids.
fn locate(project: &Project, needle: &str) -> Result<Held, Trouble> {
    match located(project, needle)? {
        (_, Some(_)) => Err(super::whole(needle)),
        (held, None) => Ok(held),
    }
}

/// [`locate`], with the part of the article a reference names after its
/// number.
fn located<'a>(project: &Project, needle: &'a str) -> Result<(Held, Option<Within<'a>>), Trouble> {
    let mut held = articles(project);
    if let Some((number, within)) = super::part_in(project, needle)? {
        return held
            .iter()
            .position(|article| article.number == Some(number))
            .map(|at| (held.swap_remove(at), within))
            .ok_or_else(|| {
                Error::NotFound {
                    what: "article",
                    named: needle.to_owned(),
                }
                .into()
            });
    }
    if let Some(at) = held.iter().position(|article| article.id == needle) {
        return Ok((held.swap_remove(at), None));
    }
    let titled: Vec<usize> = held
        .iter()
        .enumerate()
        .filter(|(_, article)| article.title.eq_ignore_ascii_case(needle))
        .map(|(at, _)| at)
        .collect();
    match titled.as_slice() {
        [at] => Ok((held.swap_remove(*at), None)),
        [] => Err(Trouble::Refused(format!(
            "no article {needle} — this project has {}",
            titles(&held)
        ))),
        _ => Err(Trouble::Refused(format!(
            "two articles are called {needle} — name the one you mean by its id"
        ))),
    }
}

/// The bytes of `markdown` a part names, and the lines they are. `named` is
/// the reference, for the refusal.
pub(crate) fn excerpt(
    markdown: &str,
    within: Within<'_>,
    named: &str,
) -> Result<(std::ops::Range<usize>, Span), Trouble> {
    let (range, _) = anchor::excerpt(markdown, within, named)?;
    Ok((range.clone(), anchor::span_of(markdown, range)))
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
    super::listing(held.iter().map(|article| article.label().to_owned()))
}
