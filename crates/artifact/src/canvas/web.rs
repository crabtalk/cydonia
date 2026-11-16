use canvas::model::Node;
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use url::Url;

pub const KIND: &str = "web";
pub const VIEWPORT: &str = "viewport";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1440,
            height: 900,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Url(Url),
    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub source: Source,
    pub viewport: Viewport,
}

pub struct Resolved {
    pub url: String,
    pub file: Option<PathBuf>,
}

impl Source {
    pub fn resolve(&self, root: &Path) -> Result<Resolved, String> {
        match self {
            Self::Url(url) => Ok(Resolved {
                url: url.to_string(),
                file: None,
            }),
            Self::File(file) => {
                let file =
                    std::path::absolute(root.join(file)).map_err(|error| error.to_string())?;
                let url = Url::from_file_path(&file)
                    .map_err(|_| "cannot make a URL for the HTML file")?;
                Ok(Resolved {
                    url: url.to_string(),
                    file: Some(file),
                })
            }
        }
    }
}

impl TryFrom<PathBuf> for Source {
    type Error = String;
    fn try_from(file: PathBuf) -> Result<Self, Self::Error> {
        if file
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err("web frame files must be relative to the project".into());
        }
        if !file
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
            })
        {
            return Err("web frame files must be HTML".into());
        }
        Ok(Self::File(file))
    }
}

impl TryFrom<&Node> for Frame {
    type Error = String;
    fn try_from(node: &Node) -> Result<Self, Self::Error> {
        if node.kind != KIND {
            return Err("not a web design frame".into());
        }
        let source = match (
            node.url.as_deref().filter(|value| !value.is_empty()),
            node.file.as_deref().filter(|value| !value.is_empty()),
        ) {
            (Some(url), None) => {
                let url =
                    Url::parse(url).map_err(|error| format!("invalid web frame URL: {error}"))?;
                if !matches!(url.scheme(), "http" | "https") {
                    return Err(
                        "web frame URLs must use http or https; use file for project HTML".into(),
                    );
                }
                Source::Url(url)
            }
            (None, Some(file)) => Source::try_from(PathBuf::from(file))?,
            _ => return Err("web frames require exactly one URL or HTML file".into()),
        };
        let viewport: Viewport = node
            .extra
            .get(VIEWPORT)
            .map(|value| {
                serde_json::from_value(value.clone())
                    .map_err(|error| format!("invalid web frame viewport: {error}"))
            })
            .transpose()?
            .unwrap_or_default();
        if !(1..=8192).contains(&viewport.width) || !(1..=8192).contains(&viewport.height) {
            return Err("web frame viewport dimensions must be between 1 and 8192".into());
        }
        Ok(Self { source, viewport })
    }
}
