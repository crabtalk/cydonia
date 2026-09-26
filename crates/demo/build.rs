//! The fences in the welcome and tour projects, painted here on the host: the
//! browser build carries no grammars. Written to `prepared.rs` as the table
//! `gui::model::language::prepare` takes.

use std::{env, fmt::Write as _, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let projects = [
        manifest.join("../gui/assets/welcome/.cydonia/articles"),
        manifest.join("assets/showcase/.cydonia/articles"),
    ];
    for articles in &projects {
        println!("cargo::rerun-if-changed={}", articles.display());
    }
    syntax_std::install();

    let mut table = String::from("const PREPARED: &[gui::model::language::Prepared] = &[\n");
    let mut documents: Vec<PathBuf> = projects
        .iter()
        .flat_map(|articles| fs::read_dir(articles).unwrap().flatten())
        .map(|entry| entry.path().join("content.md"))
        .filter(|path| path.is_file())
        .collect();
    documents.sort();
    for document in documents {
        let text = fs::read_to_string(&document).unwrap();
        for (language, source) in fences(&text) {
            if language == "markdown" {
                continue;
            }
            let Some(spans) = syntax::highlight(&source, &language) else {
                println!("cargo::warning=no grammar paints a {language} fence");
                continue;
            };
            write!(table, "    ({language:?}, {source:?}, &[").unwrap();
            for (range, kind) in spans {
                write!(
                    table,
                    "({}, {}, bezel::theme::HighlightKind::{kind:?}), ",
                    range.start, range.end
                )
                .unwrap();
            }
            table.push_str("]),\n");
        }
    }
    table.push_str("];\n");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("prepared.rs");
    fs::write(out, table).unwrap();
}

/// Every fenced block that names a language, as that name and its body.
fn fences(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut open: Option<(String, Vec<&str>)> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some((language, body)) = open.take() {
            match trimmed.starts_with("```") {
                true => found.push((language, body.join("\n"))),
                false => {
                    let mut body = body;
                    body.push(line);
                    open = Some((language, body));
                }
            }
        } else if let Some(language) = trimmed.strip_prefix("```")
            && !language.trim().is_empty()
        {
            open = Some((language.trim().to_owned(), Vec::new()));
        }
    }
    found
}
