//! Where cydonia keeps its own files outside any project, and the copies a
//! migration takes before it rewrites anything.
//!
//! A migration copies each file it is about to replace into
//! `<data>/backup/<version>/`, `<version>` being the release whose format the
//! file is in (`v0_1_26`):
//!
//! ```text
//! backup/v0_1_26/projects/<encoded project path>/path      # the project's path
//! backup/v0_1_26/projects/<encoded project path>/<file under .cydonia/>
//! backup/v0_1_26/config/<file under the config dir>
//! ```
//!
//! A file already in the backup is not copied again, so the copy is always the
//! one taken before the first rewrite. Restoring is copying the files back
//! over the project or the config directory.

use anyhow::{Context as _, Result};
use std::path::{Path, PathBuf};

/// Cydonia's data directory: `$XDG_DATA_HOME/cydonia`, defaulting to
/// `~/.local/share/cydonia`. Installed agents, caches and backups.
pub fn data_dir() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return Ok(PathBuf::from(xdg).join("cydonia"));
    }
    Ok(home()?.join(".local").join("share").join("cydonia"))
}

/// Cydonia's config directory: `$XDG_CONFIG_HOME/cydonia`, defaulting to
/// `~/.config/cydonia` — on macOS too.
pub fn config_dir() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Ok(PathBuf::from(xdg).join("cydonia"));
    }
    Ok(home()?.join(".config").join("cydonia"))
}

/// The home every directory above hangs off — except under a test, where it is
/// a directory of this process's own: a suite run against the real home
/// replaces the project list of whoever ran it (user report).
///
/// `NEXTEST` is set by the runner in every test process, which is what reaches
/// the integration binaries — they link this crate compiled without `cfg(test)`
/// and see none of it otherwise.
fn home() -> Result<PathBuf> {
    if cfg!(test) || std::env::var_os("NEXTEST").is_some() {
        return Ok(std::env::temp_dir().join(format!("cydonia-test-home-{}", std::process::id())));
    }
    dirs::home_dir().context("no home directory on this system")
}

/// A project's path as one directory name: separators become `-`.
pub fn encode(project: &Path) -> String {
    project
        .to_string_lossy()
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '-',
            ch => ch,
        })
        .collect()
}

/// Where `version`'s backup of `project` is.
pub fn project_dir(version: &str, project: &Path) -> Result<PathBuf> {
    Ok(data_dir()?
        .join("backup")
        .join(version)
        .join("projects")
        .join(encode(project)))
}

/// Copy `<project>/.cydonia/<file>` into `version`'s backup of the project,
/// unless the backup already holds it. A file that is not there is nothing to
/// keep.
pub fn keep(version: &str, project: &Path, file: &Path) -> Result<()> {
    let source = crate::project::fs::Project::new(project)
        .cydonia()
        .join(file);
    if !source.is_file() {
        return Ok(());
    }
    let dir = project_dir(version, project)?;
    let target = dir.join(file);
    if target.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let path = dir.join("path");
    if !path.exists() {
        std::fs::write(&path, project.to_string_lossy().as_bytes())?;
    }
    std::fs::copy(&source, &target)?;
    Ok(())
}

/// Copy `<config dir>/<file>` into `version`'s backup of the config, unless
/// the backup already holds it.
pub fn keep_config(version: &str, file: &str) -> Result<()> {
    let source = config_dir()?.join(file);
    if !source.is_file() {
        return Ok(());
    }
    let target = data_dir()?
        .join("backup")
        .join(version)
        .join("config")
        .join(file);
    if target.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&source, &target)?;
    Ok(())
}

/// What one release's backup holds.
#[derive(Debug, Clone)]
pub struct Backup {
    /// The directory name: the release whose files these are, `v0_1_26`.
    pub version: String,
    pub dir: PathBuf,
    /// The projects it holds files of, by their paths.
    pub projects: Vec<PathBuf>,
    /// The config files it holds, by their names.
    pub config: Vec<String>,
    pub bytes: u64,
    /// When it was taken: its directory's modification time.
    pub taken: Option<std::time::SystemTime>,
}

impl Backup {
    /// `v0_1_26` as a release is spelled, `0.1.26`.
    pub fn release(&self) -> String {
        self.version.trim_start_matches('v').replace('_', ".")
    }
}

/// Whether the data directory holds any backup. Cheaper than [`list`]: it
/// reads one directory and sizes nothing.
pub fn any() -> bool {
    data_dir()
        .ok()
        .and_then(|dir| std::fs::read_dir(dir.join("backup")).ok())
        .is_some_and(|mut entries| {
            entries.any(|entry| entry.is_ok_and(|entry| entry.path().is_dir()))
        })
}

/// Every backup in the data directory, most recently taken first.
pub fn list() -> Vec<Backup> {
    let Ok(root) = data_dir().map(|dir| dir.join("backup")) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut found: Vec<Backup> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| dir.is_dir())
        .map(|dir| {
            let projects = std::fs::read_dir(dir.join("projects"))
                .map(|entries| {
                    entries
                        .flatten()
                        .filter_map(|entry| std::fs::read_to_string(entry.path().join("path")).ok())
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_default();
            let config = std::fs::read_dir(dir.join("config"))
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            Backup {
                version: dir
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                projects,
                config,
                bytes: size(&dir),
                taken: std::fs::metadata(&dir)
                    .and_then(|meta| meta.modified())
                    .ok(),
                dir,
            }
        })
        .collect();
    found.sort_by_key(|backup| std::cmp::Reverse(backup.taken));
    found
}

/// Delete one release's backup.
pub fn remove(version: &str) -> Result<()> {
    let dir = data_dir()?.join("backup").join(version);
    anyhow::ensure!(
        dir.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("backup")),
        "not a backup: {version}"
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

/// Put `version`'s files back where they were taken from, and take away what
/// the migrations made of them: `state.db` where `entries.db` comes back, and
/// the directory of each board that comes back as a flat file. Run with no
/// cydonia reading the projects — see the `--restore-backup` mode of the
/// binary.
pub fn restore(version: &str) -> Result<()> {
    let dir = data_dir()?.join("backup").join(version);
    if let Ok(entries) = std::fs::read_dir(dir.join("projects")) {
        for kept in entries.flatten().map(|entry| entry.path()) {
            let Ok(project) = std::fs::read_to_string(kept.join("path")) else {
                continue;
            };
            let cydonia = crate::project::fs::Project::new(project.trim()).cydonia();
            put_back(&kept, &kept, &cydonia)?;
            if kept.join("entries.db").is_file() {
                let _ = std::fs::remove_file(cydonia.join("state.db"));
            }
            if let Ok(boards) = std::fs::read_dir(kept.join("boards")) {
                for flat in boards.flatten().map(|entry| entry.path()) {
                    if let Some(id) = flat.file_stem() {
                        let _ = std::fs::remove_dir_all(cydonia.join("boards").join(id));
                    }
                }
            }
            if let Ok(id) = std::fs::read_to_string(kept.join(LEGACY_BOARD_ID)) {
                let _ = std::fs::remove_dir_all(cydonia.join("boards").join(id.trim()));
            }
        }
    }
    let config = dir.join("config");
    if config.is_dir() {
        put_back(&config, &config, &config_dir()?)?;
    }
    Ok(())
}

/// The file beside a project's backup naming the board the legacy
/// `board.toml` became, so a restore can take that board away again.
pub const LEGACY_BOARD_ID: &str = "legacy-board-id";

/// Note in `version`'s backup of `project` which board the legacy
/// `board.toml` became.
pub fn note_legacy_board(version: &str, project: &Path, id: &str) -> Result<()> {
    let dir = project_dir(version, project)?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(LEGACY_BOARD_ID), id)?;
    Ok(())
}

/// Copy every file under `from` to the same place under `to`, but the
/// bookkeeping a backup keeps beside the files.
fn put_back(root: &Path, from: &Path, to: &Path) -> Result<()> {
    for entry in std::fs::read_dir(from)?.flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(root)?;
        if from == root && (rel == Path::new("path") || rel == Path::new(LEGACY_BOARD_ID)) {
            continue;
        }
        if path.is_dir() {
            put_back(root, &path, to)?;
        } else {
            let target = to.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn size(path: &Path) -> u64 {
    match std::fs::read_dir(path) {
        Ok(entries) => entries.flatten().map(|entry| size(&entry.path())).sum(),
        Err(_) => std::fs::metadata(path).map_or(0, |meta| meta.len()),
    }
}
