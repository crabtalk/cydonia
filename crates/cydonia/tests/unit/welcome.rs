//! The seed a first run writes: what it holds, and the order it lands in.

use super::*;
use artifact::project::Project as _;
use std::collections::BTreeSet;

/// Where the content is checked in, which is also what [`SEED`] embeds.
fn source() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/welcome/.cydonia"
    ))
}

/// A directory of this test's own, emptied first so a rerun starts clean.
fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "cydonia-welcome-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// Every file of the checked-in project, by its path under `.cydonia/`.
/// Databases are written by the app and the ignore file is the repository's
/// own — neither is content, and neither is shipped.
fn checked_in(dir: &Path, prefix: &str, found: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).expect("welcome content").flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let name = match prefix.is_empty() {
            true => name.to_string(),
            false => format!("{prefix}/{name}"),
        };
        if path.is_dir() {
            checked_in(&path, &name, found);
        } else if !name.ends_with(".db") && !name.ends_with(".gitignore") {
            found.insert(name);
        }
    }
}

/// A file added to the project and not to [`SEED`] would ship as nothing at
/// all: the list is written out by hand, because it is also what fixes the
/// order the entries are read in.
#[test]
fn the_seed_holds_every_file_of_the_checked_in_project() {
    let mut found = BTreeSet::new();
    checked_in(&source(), "", &mut found);
    let listed: BTreeSet<String> = SEED
        .iter()
        .flat_map(|entry| entry.iter())
        .map(|(name, _)| (*name).to_string())
        .collect();
    assert_eq!(found, listed);
}

/// The one thing a copy cannot carry and a filesystem will not settle: entries
/// are listed newest first, so the seed is written oldest first and the sidebar
/// reads top to bottom.
#[test]
fn the_seeded_project_reads_in_order() {
    let project = scratch("order");
    write(&project).expect("seed written");

    let titles: Vec<String> = crate::model::article::list(&project)
        .into_iter()
        .map(|article| article.title)
        .collect();
    assert_eq!(titles, ["Start here", "Writing in Cydonia"]);

    // The board is older than both, so it sits under them.
    let boards = fs::Project::new(&project).boards();
    let board = boards.first().expect("the welcome board");
    assert_eq!(board.name, "Getting started");
    let newest = crate::model::article::list(&project)
        .last()
        .expect("an article")
        .touched;
    assert!(
        board.touched < newest,
        "the board must be older than every article"
    );
}

/// `init` writes the ignore file a project carries. The copy checked in beside
/// the content ignores databases only, so that it can be committed at all, and
/// shipping that one would leave a seeded project visible to a repository.
#[test]
fn the_seed_carries_the_projects_own_ignore_file() {
    let project = scratch("ignore");
    write(&project).expect("seed written");
    let ignore = std::fs::read_to_string(project.join(".cydonia/.gitignore")).expect("ignore file");
    assert_eq!(ignore, "*\n");
}

/// The landing entry is the newest one written, so the article a first run
/// opens on is also the one at the top of the sidebar.
#[test]
fn the_landing_entry_is_the_last_one_written() {
    let newest = SEED.last().expect("a seed");
    assert!(
        newest.iter().any(|(name, _)| *name == LANDING),
        "{LANDING} is not part of the newest entry"
    );
}
