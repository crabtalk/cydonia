//! Small helpers with no home of their own.

use anyhow::{Result, bail};
use std::path::{Component, Path, PathBuf};

/// `Command` for `program` as [`which::which`] found it. A Windows program is
/// run through the file its extension names — `npm.cmd` through `cmd.exe`,
/// which the standard library arranges — and without a console window of its
/// own, which a GUI process would otherwise be handed for every child it
/// starts.
#[cfg(feature = "install")]
pub(crate) fn command(program: &Path) -> std::process::Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
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

/// The package name in a spec like `@scope/name@1.2.3` — the version separator
/// is the last `@` that isn't the scope's leading one.
pub fn package_name(spec: &str) -> &str {
    match spec.rfind('@') {
        Some(ix) if ix > 0 => &spec[..ix],
        _ => spec,
    }
}
