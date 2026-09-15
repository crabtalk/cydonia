use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize)]
struct Metadata {
    name: String,
    description: String,
}

pub fn generate(root: &Path) -> Result<String, String> {
    let mut resources = BTreeMap::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let file = path;
        let source = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let source = source.replace("\r\n", "\n");
        let (frontmatter, body) = source
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .ok_or_else(|| format!("{}: missing YAML frontmatter", file.display()))?;
        let meta: Metadata =
            serde_yaml_ng::from_str(frontmatter).map_err(|e| format!("{}: {e}", file.display()))?;
        if meta.name.is_empty()
            || meta.name.len() > 64
            || !meta
                .name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || meta.name.starts_with('-')
            || meta.name.ends_with('-')
            || meta.name.contains("--")
            || file.file_stem().and_then(|name| name.to_str()) != Some(meta.name.as_str())
        {
            return Err(format!(
                "{}: invalid name or filename mismatch",
                file.display()
            ));
        }
        if meta.description.trim().is_empty() || body.trim().is_empty() {
            return Err(format!(
                "{}: description and instructions must not be empty",
                file.display()
            ));
        }
        let description = meta
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let entry = format!(
            "Resource {{ name: {:?}, description: {:?}, content: {:?} }}",
            meta.name, description, source,
        );
        if resources.insert(meta.name, entry).is_some() {
            return Err("duplicate resource name".to_owned());
        }
    }
    Ok(format!(
        "static BUILTINS: &[Resource] = &[{}];",
        resources.into_values().collect::<Vec<_>>().join(",\n")
    ))
}
