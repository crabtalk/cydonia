use crate::{label, project, stamp};
use canvas::model::Canvas as Document;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    ops::{Deref, DerefMut},
    str::FromStr,
};
pub const DIR: &str = "canvases";
pub const FILE: &str = "content.canvas";
pub const UNNAMED: &str = "Untitled";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Properties {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Canvas {
    #[serde(skip)]
    pub id: String,
    #[serde(default, rename = "cydonia")]
    pub properties: Properties,
    #[serde(flatten)]
    pub document: Document,
    #[serde(skip)]
    pub number: Option<u64>,
    #[serde(skip)]
    pub touched: u128,
    #[serde(skip)]
    pub created: Option<u128>,
    #[serde(skip)]
    pub version: Option<String>,
}

impl Deref for Canvas {
    type Target = Properties;
    fn deref(&self) -> &Properties {
        &self.properties
    }
}
impl DerefMut for Canvas {
    fn deref_mut(&mut self) -> &mut Properties {
        &mut self.properties
    }
}

impl Canvas {
    pub fn new(id: String, title: &str, document: Document) -> Self {
        let created = stamp::named(&id);
        Self {
            id,
            properties: Properties {
                title: title.to_owned(),
                ..Properties::default()
            },
            document,
            number: None,
            touched: stamp::now(),
            created,
            version: None,
        }
    }

    pub fn label(&self) -> &str {
        if self.title.trim().is_empty() {
            UNNAMED
        } else {
            &self.title
        }
    }
}

impl FromStr for Canvas {
    type Err = serde_json::Error;
    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let mut canvas: Self = serde_json::from_str(source)?;
        canvas.labels = label::normalize_all(&canvas.labels);
        canvas.version = Some(project::version(source.as_bytes()));
        Ok(canvas)
    }
}

impl fmt::Display for Canvas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&serde_json::to_string_pretty(self).map_err(|_| fmt::Error)?)
    }
}
