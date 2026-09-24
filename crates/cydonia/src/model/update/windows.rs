//! The Windows half of the updater: the release's installer, run silently on
//! the way out.
//!
//! The installer is unsigned. What is fetched is trusted as far as the HTTPS
//! connection to GitHub's release host.

use anyhow::{Context as _, Result};
use bezel::gpui::App;
use std::path::{Path, PathBuf};

pub(super) const SUPPORTED: bool = cfg!(target_arch = "x86_64");

/// The installer's name after the version. The release file itself carries no
/// version, so the version is added for the copy kept on disk.
pub(super) const SUFFIX: &str = "-windows-x86_64-setup.exe";

/// The release file, as `bundle/windows/cydonia.iss` names it.
pub(super) fn remote(_: &str) -> String {
    "cydonia-windows-x86_64-setup.exe".into()
}

/// The directory this process runs from, when the installer put it there: Inno
/// Setup leaves `unins000.exe` beside what it installed. A bare exe out of a
/// zip or `cargo install` is a build the installer would not replace.
pub(super) fn bundle(_: &App) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    dir.join("unins000.exe")
        .is_file()
        .then(|| dir.to_path_buf())
}

/// The installer, downloaded. Nothing is unpacked: running it is the swap.
pub(super) fn stage(version: &str, _: &Path) -> Result<PathBuf> {
    super::download(version)
}

/// Start the installer and leave it to close this process and relaunch the new
/// one — `/RELAUNCH` is `cydonia.iss`'s own switch for that.
pub(super) fn swap_on_exit(_: &Path, setup: &Path) -> Result<()> {
    std::process::Command::new(setup)
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/CLOSEAPPLICATIONS",
            "/RELAUNCH",
        ])
        .spawn()
        .context("the installer did not start")?;
    Ok(())
}
