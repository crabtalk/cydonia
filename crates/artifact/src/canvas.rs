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

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    #[serde(default)]
    pub nodes: Vec<canvas::model::Node>,
    #[serde(default)]
    pub edges: Vec<canvas::model::Edge>,
    #[serde(default)]
    pub remove_nodes: Vec<String>,
    #[serde(default)]
    pub remove_edges: Vec<String>,
}

impl Edit {
    pub fn apply(self, canvas: &mut Canvas) -> Result<(), String> {
        let mut next = canvas.document.clone();
        next.nodes
            .retain(|node| !self.remove_nodes.contains(&node.id));
        next.edges.retain(|edge| {
            !self.remove_edges.contains(&edge.id)
                && !self.remove_nodes.contains(&edge.from_node)
                && !self.remove_nodes.contains(&edge.to_node)
        });
        for node in self.nodes {
            match next.nodes.iter().position(|held| held.id == node.id) {
                Some(at) => next.nodes[at] = node,
                None => next.nodes.push(node),
            }
        }
        for edge in self.edges {
            match next.edges.iter().position(|held| held.id == edge.id) {
                Some(at) => next.edges[at] = edge,
                None => next.edges.push(edge),
            }
        }
        let mut ids = std::collections::HashSet::new();
        for node in &next.nodes {
            if node.id.is_empty() || !ids.insert(&node.id) {
                return Err(format!("node id must be nonempty and unique: {}", node.id));
            }
            if node.width <= 0 || node.height <= 0 {
                return Err(format!(
                    "node {} must have positive width and height",
                    node.id
                ));
            }
        }
        for edge in &next.edges {
            if edge.id.is_empty() || !ids.insert(&edge.id) {
                return Err(format!("edge id must be nonempty and unique: {}", edge.id));
            }
            if next.node(&edge.from_node).is_none() || next.node(&edge.to_node).is_none() {
                return Err(format!("edge {} must connect existing nodes", edge.id));
            }
        }
        canvas.document = next;
        Ok(())
    }
}
