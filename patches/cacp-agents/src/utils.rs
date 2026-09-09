//! Small helpers with no home of their own.

use anyhow::{Result, bail};
use std::path::{Component, Path, PathBuf};

/// The first `program` on `PATH`.
///
/// On Windows, under any of the extensions `PATHEXT` names — the lookup
/// `cmd.exe` does, which is what finds `npm.cmd` for `npm` and `tar.exe` for
/// `tar`. The bare name is tried last there: a file under it is a script for
/// a shell this machine may not have. (cydonia: Windows support.)
pub(crate) fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts = extensions();
    std::env::split_paths(&path).find_map(|dir| {
        let base = dir.join(program);
        let with = |ext: &String| {
            let mut with = base.as_os_str().to_owned();
            with.push(ext);
            PathBuf::from(with)
        };
        exts.iter()
            .map(with)
            .chain(std::iter::once(base.clone()))
            .find(|candidate| candidate.is_file())
    })
}

/// `PATHEXT` as the shell reads it, or its usual default where it is unset.
/// Nothing off Windows, where a program is the file its name is.
fn extensions() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    std::env::var("PATHEXT")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned())
        .split(';')
        .filter(|ext| ext.starts_with('.'))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// `Command` for `program` as [`which`] found it. A Windows program is run
/// through the file its extension names — `npm.cmd` through `cmd.exe`, which
/// the standard library arranges — and without a console window of its own,
/// which a GUI process would otherwise be handed for every child it starts.
/// (cydonia: Windows support.)
pub(crate) fn command(program: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// `dir` joined with a relative path the registry supplied — at least one
/// component, every one a plain name. An id or a `cmd` that climbs out, or an
/// empty one naming `dir` itself, would aim an install — or the
/// `remove_dir_all` that undoes one — somewhere it was never given.
pub fn contained(dir: &Path, rel: &str) -> Result<PathBuf> {
    let rel = Path::new(rel.trim_start_matches("./"));
    let mut parts = rel.components().peekable();
    if parts.peek().is_none() || !parts.all(|c| matches!(c, Component::Normal(_))) {
        bail!("{} is not a path inside {}", rel.display(), dir.display());
    }
    Ok(dir.join(rel))
}
