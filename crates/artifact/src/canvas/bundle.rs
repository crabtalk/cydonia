use crate::fs;
use canvas::{json_canvas, model::Canvas};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub struct Bundle {
    pub document: Canvas,
    files: Vec<(String, PathBuf, Vec<u8>)>,
}

impl Bundle {
    pub fn read(source: &Path) -> anyhow::Result<Self> {
        let document = json_canvas::import(&std::fs::read_to_string(source)?)?;
        Self::collect(document, source.parent().unwrap_or(Path::new(".")))
    }

    pub fn collect(document: Canvas, root: &Path) -> anyhow::Result<Self> {
        let mut files = Vec::new();
        for reference in document
            .nodes
            .iter()
            .flat_map(|node| [node.file.as_ref(), node.background.as_ref()])
            .flatten()
        {
            if url::Url::parse(reference).is_ok()
                || files.iter().any(|(held, _, _)| held == reference)
            {
                continue;
            }
            let path = root.join(reference);
            let bytes = std::fs::read(&path)
                .map_err(|error| anyhow::anyhow!("cannot read {}: {error}", path.display()))?;
            files.push((reference.clone(), path, bytes));
        }
        Ok(Self { document, files })
    }

    pub fn write_assets(&mut self, root: &Path, relative: &Path) -> anyhow::Result<()> {
        let mut replaced = HashMap::new();
        for (at, (reference, source, bytes)) in self.files.iter().enumerate() {
            let name = source
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("asset has no filename"))?
                .to_string_lossy();
            let path = relative.join(format!("{at}-{name}"));
            fs::save(&root.join(&path), bytes)?;
            replaced.insert(reference.clone(), path.to_string_lossy().into_owned());
        }
        for node in &mut self.document.nodes {
            for reference in [&mut node.file, &mut node.background].into_iter().flatten() {
                if let Some(path) = replaced.get(reference) {
                    *reference = path.clone();
                }
            }
        }
        Ok(())
    }

    pub fn export(mut self, destination: &Path) -> anyhow::Result<()> {
        let root = destination.parent().unwrap_or(Path::new("."));
        let stem = destination
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let relative = if self.files.is_empty() {
            None
        } else {
            let mut at = 0;
            loop {
                let name = if at == 0 {
                    format!("{stem}.assets")
                } else {
                    format!("{stem}-{at}.assets")
                };
                match std::fs::create_dir(root.join(&name)) {
                    Ok(()) => break Some(PathBuf::from(name)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => at += 1,
                    Err(error) => return Err(error.into()),
                }
            }
        };
        let result = (|| {
            if let Some(relative) = &relative {
                self.write_assets(root, relative)?;
            }
            self.document.extra.remove("cydonia");
            fs::save(destination, json_canvas::export(&self.document))?;
            Ok(())
        })();
        if result.is_err()
            && let Some(relative) = relative
        {
            let _ = std::fs::remove_dir_all(root.join(relative));
        }
        result
    }
}
