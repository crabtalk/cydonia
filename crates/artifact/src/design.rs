use crate::{
    comment::{Anchor, ElementComment, Thread},
    stamp,
    web::Viewport,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DIR: &str = "designs";
pub const FILE: &str = "design.toml";
pub const INDEX: &str = "index.html";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Design {
    #[serde(skip)]
    pub id: String,
    pub title: String,
    pub labels: Vec<String>,
    pub archived: bool,
    pub viewport: Viewport,
    #[serde(alias = "pages")]
    pub artboards: Vec<Artboard>,
    #[serde(skip)]
    pub number: Option<u64>,
    #[serde(skip)]
    pub version: Option<String>,
    #[serde(skip)]
    pub touched: u128,
    #[serde(skip)]
    pub revisions_loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artboard {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<ElementComment>,
    /// Version of the files used to invalidate a rendered preview.
    #[serde(skip)]
    pub revision: Option<String>,
}

impl Artboard {
    pub fn comment(&mut self, anchor: Anchor, note: String) -> u64 {
        let id = Thread::fresh_id(
            stamp::now() as u64,
            self.comments.iter().map(|comment| comment.thread.id),
        );
        self.comments.push(ElementComment {
            anchor,
            thread: Thread::new(id, note, None),
        });
        id
    }

    pub fn path(&self) -> PathBuf {
        PathBuf::from(&self.id).join(INDEX)
    }
}

impl Design {
    pub fn label(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            "Untitled design"
        } else {
            title
        }
    }

    pub fn created(&self) -> Option<u128> {
        stamp::named(&self.id)
    }
}
