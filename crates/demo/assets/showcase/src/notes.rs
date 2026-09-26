/// Release notes for `version`, grouped by the kind each commit names.
pub fn draft(version: &str, commits: &[&str]) -> String {
    let mut features = Vec::new();
    let mut fixes = Vec::new();
    for commit in commits {
        match commit.split_once(':') {
            Some((kind, text)) if kind.starts_with("feat") => features.push(text.trim()),
            Some((kind, text)) if kind.starts_with("fix") => fixes.push(text.trim()),
            _ => {}
        }
    }
    let mut out = format!("# {version}\n");
    for (heading, lines) in [("Features", features), ("Fixes", fixes)] {
        if lines.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {heading}\n\n"));
        for line in lines {
            out.push_str(&format!("- {line}\n"));
        }
    }
    out
}
