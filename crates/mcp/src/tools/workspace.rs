//! What the cydonia window is showing.

use crate::{
    rail::{self, Shown},
    tool::{Answer, Args, Outcome, Tool},
    tools::fields,
};
use artifact::article;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub static TOOLS: [Tool; 1] = [Tool {
    name: "workspace_focus",
    description: "The articles, boards and tables the cydonia window is showing now: one entry, \
        or every pane of an open space with the focused one marked. Chats are left out. What \
        \"this article\" or \"this board\" means when a request does not name one.",
    schema: |_| fields(false, &[]),
    writes: false,
    deletes: false,
    call: focus,
}];

fn focus(_: Args<'_>) -> Outcome {
    let shown: Vec<Value> = rail::shown().iter().map(described).collect();
    let text = match on_screen() {
        Some(text) => text,
        None => "the cydonia window is showing no article, board or table".to_owned(),
    };
    Ok(Answer::said(text).with(json!({ "shown": shown })))
}

/// The entries on screen, a line each, or `None` when there are none.
pub fn on_screen() -> Option<String> {
    let shown = rail::shown();
    if shown.is_empty() {
        return None;
    }
    let lines = shown
        .iter()
        .map(|entry| {
            let about = described(entry);
            let number = about["number"]
                .as_u64()
                .map(|number| format!("#{number} "))
                .unwrap_or_default();
            let path = about["path"]
                .as_str()
                .map(|path| format!(", at {path}"))
                .unwrap_or_default();
            format!(
                "- {number}[{}] {} in {}{path}{}",
                entry.kind,
                about["title"].as_str().unwrap_or(&entry.id),
                entry.project.display(),
                if entry.focused && shown.len() > 1 {
                    " (focused)"
                } else {
                    ""
                },
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some(lines)
}

/// One entry with what the project's files say about it: its number and title,
/// and an article's `content.md`.
fn described(shown: &Shown) -> Value {
    let entry = artifact::entry::list(&shown.project)
        .ok()
        .and_then(|entries| {
            entries
                .into_iter()
                .find(|entry| entry.kind == shown.kind && entry.id == shown.id)
        });
    let content = (shown.kind == "article")
        .then(|| article_content(&shown.project, &shown.id))
        .flatten();
    json!({
        "project": shown.project,
        "kind": shown.kind,
        "id": shown.id,
        "number": entry.as_ref().map(|entry| entry.number),
        "title": entry.as_ref().map(|entry| entry.title.clone()),
        "focused": shown.focused,
        "path": content,
    })
}

fn article_content(project: &Path, id: &str) -> Option<PathBuf> {
    std::fs::read_dir(article::dir(project))
        .ok()?
        .filter_map(|item| item.ok())
        .map(|item| article::content(&item.path()))
        .find(|content| content.is_file() && article::id_of(content) == id)
}
