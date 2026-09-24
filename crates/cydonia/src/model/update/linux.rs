//! The Linux half of the updater: the release tarball unpacked beside the
//! installed tree and swapped in after exit.
//!
//! The tarball is unsigned. What is fetched is trusted as far as the HTTPS
//! connection to GitHub's release host.

use super::run;
use anyhow::{Context as _, Result, bail};
use bezel::gpui::App;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// The architectures a tarball is cut for, spelled as rustc and `uname -m`
/// both spell them.
const ARCH: &str = if cfg!(target_arch = "aarch64") {
    "aarch64"
} else {
    "x86_64"
};

pub(super) const SUPPORTED: bool = cfg!(any(target_arch = "x86_64", target_arch = "aarch64"));

/// The tarball's name after the version. The release file itself carries no
/// version, so the version is added for the copy kept on disk.
pub(super) const SUFFIX: &str = if cfg!(target_arch = "aarch64") {
    "-linux-aarch64.tar.gz"
} else {
    "-linux-x86_64.tar.gz"
};

/// The release file, as `make tarball` names it.
pub(super) fn remote(_: &str) -> String {
    format!("cydonia-linux-{ARCH}.tar.gz")
}

/// The unpacked tarball this process runs from: `…/cydonia.app/bin/cydonia`,
/// which is what install.sh lays out. Any other layout is a build a release
/// cannot replace.
pub(super) fn bundle(_: &App) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let bin = exe.parent()?;
    let root = bin.parent()?;
    (bin.file_name()? == "bin" && root.extension()? == "app").then(|| root.to_path_buf())
}

/// Unpack `version` beside the installed tree, and answer with the tree.
pub(super) fn stage(version: &str, app: &Path) -> Result<PathBuf> {
    let parent = app
        .parent()
        .context("the app is at the root of the filesystem")?;
    // Beside the tree it replaces, because the swap is a rename and a rename
    // does not cross filesystems.
    let staging = parent.join(format!("{}{version}", super::STAGING));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("{} cannot be written to", parent.display()))?;

    let unpacked = (|| -> Result<PathBuf> {
        let image = super::download(version)?;
        run(
            "tar",
            [
                OsStr::new("-xzf"),
                image.as_os_str(),
                OsStr::new("-C"),
                staging.as_os_str(),
            ],
        )?;
        let staged = staging.join("cydonia.app");
        if !staged.join("bin/cydonia").is_file() {
            bail!("the release holds no cydonia.app/bin/cydonia");
        }
        Ok(staged)
    })();
    if unpacked.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    unpacked
}

/// Swap the staged tree in once this process is gone, and start it detached
/// from the script.
pub(super) fn swap_on_exit(app: &Path, staged: &Path) -> Result<()> {
    super::swap_on_exit(
        app,
        staged,
        r#"nohup "$app/bin/cydonia" > /dev/null 2>&1 &"#,
    )
}
