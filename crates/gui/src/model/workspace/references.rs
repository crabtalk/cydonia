//! What a written reference, `[project]#ref[:from[-to]]`, names among the
//! open projects — see [`artifact::reference`].

use super::{Showing, Workspace};
use crate::model::project::Project;
use artifact::{
    reference::{self, Target, Turns},
    search::Kind,
};
use bezel::gpui::{App, Entity, Global, WeakEntity};
use std::path::PathBuf;

/// An entry a reference resolved to.
pub struct Resolved {
    pub project: PathBuf,
    pub showing: Showing,
    pub kind: Kind,
    pub number: u64,
    pub turns: Option<Turns>,
    /// The entry's own title, empty where it has none.
    pub title: String,
}

/// The workspace, for readers handed only `&App` — a link preview.
struct Current(WeakEntity<Workspace>);

impl Global for Current {}

impl Workspace {
    /// Make this workspace the one [`resolve_in`] reads.
    pub fn install(this: &Entity<Self>, cx: &mut App) {
        cx.set_global(Current(this.downgrade()));
    }

    /// The entry a reference names, or why it names none. A reference with
    /// no project is read in the active one.
    pub fn resolve(&self, text: &str) -> Result<Resolved, String> {
        let Some(reference) = reference::parse(text) else {
            return Err(format!("{text} is not a reference"));
        };
        let Target::Entry { number, turns } = reference.target else {
            return Err(format!("{text} is a card, not an entry"));
        };
        let Some(project) = self.named_project(reference.project) else {
            return Err(format!("No open project for {text}"));
        };
        let found = project
            .sessions
            .iter()
            .find(|chat| chat.number == Some(number))
            .map(|chat| (Kind::Session, Showing::Session(chat.id), chat.title.clone()))
            .or_else(|| {
                project
                    .articles
                    .iter()
                    .find(|article| article.number == Some(number))
                    .map(|article| {
                        (
                            Kind::Article,
                            Showing::Article(article.id.clone()),
                            article.title.clone(),
                        )
                    })
            })
            .or_else(|| {
                project
                    .boards
                    .iter()
                    .find(|board| board.number == Some(number))
                    .map(|board| {
                        (
                            Kind::Board,
                            Showing::Board(board.id.clone()),
                            board.name.clone(),
                        )
                    })
            });
        let Some((kind, showing, title)) = found else {
            return Err(format!("Nothing is {text}"));
        };
        Ok(Resolved {
            project: project.path.clone(),
            showing,
            kind,
            number,
            turns,
            title,
        })
    }

    fn named_project(&self, name: Option<&str>) -> Option<&Project> {
        match name {
            Some(name) => self
                .projects
                .iter()
                .find(|project| project.path.file_name().is_some_and(|last| last == name)),
            None => self.active_project(),
        }
    }
}

/// [`Workspace::resolve`] on the installed workspace.
pub fn resolve_in(text: &str, cx: &App) -> Option<Resolved> {
    let workspace = cx.try_global::<Current>()?.0.upgrade()?;
    workspace.read(cx).resolve(text).ok()
}
