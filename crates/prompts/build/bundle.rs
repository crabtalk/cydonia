use skills::Frontmatter;
use std::{collections::BTreeMap, fs, path::Path};

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
        let (meta, body) = Frontmatter::parse(&source)
            .ok_or_else(|| format!("{}: missing YAML frontmatter", file.display()))?
            .map_err(|e| format!("{}: {e}", file.display()))?;
        let name = meta.name.clone().unwrap_or_default();
        if !Frontmatter::valid_name(&name)
            || file.file_stem().and_then(|stem| stem.to_str()) != Some(name.as_str())
        {
            return Err(format!(
                "{}: invalid name or filename mismatch",
                file.display()
            ));
        }
        let description = meta.summary().unwrap_or_default();
        if description.is_empty() || body.trim().is_empty() {
            return Err(format!(
                "{}: description and instructions must not be empty",
                file.display()
            ));
        }
        let entry = format!(
            "Resource {{ name: {:?}, description: {:?}, content: {:?} }}",
            name, description, source,
        );
        if resources.insert(name, entry).is_some() {
            return Err("duplicate resource name".to_owned());
        }
    }
    Ok(format!(
        "static BUILTINS: &[Resource] = &[{}];",
        resources.into_values().collect::<Vec<_>>().join(",\n")
    ))
}
