use crate::{comment::ElementComment, stamp};
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
    pub pages: Vec<Page>,
    #[serde(skip)]
    pub number: Option<u64>,
    #[serde(skip)]
    pub version: Option<String>,
    #[serde(skip)]
    pub touched: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<Review>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<ElementComment>,
    #[serde(skip)]
    pub revision: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub revision: String,
    pub status: Status,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    New,
    Approved,
    Changes,
}

impl Page {
    pub fn status(&self) -> Status {
        self.review
            .as_ref()
            .filter(|review| Some(&review.revision) == self.revision.as_ref())
            .map_or(Status::New, |review| review.status)
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
