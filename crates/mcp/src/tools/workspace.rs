//! What the cydonia window is showing.

use crate::{
    rail::{self, Shown},
    tool::{Answer, Args, Outcome, Tool},
    tools::{Project, fields},
};
use artifact::{article, kind::Kind, project::Project as _};
use serde_json::{Value, json};

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
                entry.project,
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

/// One entry with what the project's store says about it: its number and
/// title, and an article's `content.md` on the project's machine.
fn described(shown: &Shown) -> Value {
    let project = Project::at(shown.project.clone()).ok();
    let entry = project
        .as_ref()
        .and_then(|project| match shown.kind {
            Kind::Table => project.store.entries().ok(),
            _ => artifact::entry::catalog(&project.store).ok(),
        })
        .and_then(|entries| {
            entries
                .into_iter()
                .find(|entry| entry.kind == shown.kind && entry.id == shown.id)
        });
    let content = project
        .filter(|_| shown.kind == Kind::Article)
        .and_then(|project| project.store.place(Kind::Article, &shown.id))
        .map(|dir| article::content(&dir));

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
