//! The macOS half of the updater: a signed, notarized `.app` out of a dmg,
//! staged beside the running bundle and swapped in after exit.
//!
//! # What is trusted
//!
//! Nothing this process downloads carries `com.apple.quarantine` — that is set
//! by browsers, not by us — so Gatekeeper will never assess the staged bundle
//! on first launch. The assessment it would have made is therefore made here,
//! before the swap is offered: the copy must verify against its own signature,
//! Apple must have notarized it, and it must be signed by whoever signed *this*
//! copy. That last one needs no certificate written down anywhere — the running
//! app is the reference.

use super::run;
use anyhow::{Context as _, Result, bail};
use bezel::gpui::App;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// Only Apple silicon has an image. `arm64` is Apple's word for the
/// architecture — `uname -m`, which is what the Makefile reads — where rustc's
/// is `aarch64`.
pub(super) const SUPPORTED: bool = cfg!(target_arch = "aarch64");

/// The image's name after the version, as the Makefile names it.
pub(super) const SUFFIX: &str = "-arm64.dmg";

/// The release file: the dmg is named after its version on the release too.
pub(super) fn remote(version: &str) -> String {
    super::asset(version)
}

/// The `.app` this process runs from. Unbundled, `app_path` answers with the
/// directory the executable sits in, which is not something to move a release
/// on top of.
pub(super) fn bundle(cx: &App) -> Option<PathBuf> {
    let path = cx.app_path().ok()?;
    (path.extension()? == "app").then_some(path)
}

/// Put a verified copy of `version` beside the running app, and answer with where
/// it is.
///
/// Ordered so that the expensive thing happens after the thing that can fail
/// for free: a bundle in a directory this user cannot write to is a refusal
/// that should not cost a download first.
pub(super) fn stage(version: &str, app: &Path) -> Result<PathBuf> {
    let parent = app.parent().context("the app is at the root of a volume")?;
    // Beside the app it replaces, because the swap is a rename and a rename
    // does not cross volumes — a home directory on another disk would put the
    // download somewhere `mv` could not move it from. Dot-prefixed so the
    // Finder keeps it out of the way while it waits.
    let staging = parent.join(format!("{}{version}", super::STAGING));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("{} cannot be written to", parent.display()))?;

    let image = super::download(version)?;
    let mount = super::cache()?.join("mount");
    let _ = std::fs::create_dir_all(&mount);
    // In case a crash left the last one attached: `attach` will not take a
    // mountpoint that is already in use, and this is the only thing that uses
    // this one.
    let _ = run(
        "/usr/bin/hdiutil",
        [
            OsStr::new("detach"),
            mount.as_os_str(),
            OsStr::new("-quiet"),
        ],
    );
    run(
        "/usr/bin/hdiutil",
        [
            OsStr::new("attach"),
            image.as_os_str(),
            OsStr::new("-nobrowse"),
            OsStr::new("-readonly"),
            OsStr::new("-noautoopen"),
            OsStr::new("-mountpoint"),
            mount.as_os_str(),
        ],
    )?;
    // Detaching is unconditional: a failure in here must not leave the image
    // mounted — the same rule the Makefile's `dmg` target follows.
    let staged = copy_out(&mount, &staging);
    let _ = run(
        "/usr/bin/hdiutil",
        [
            OsStr::new("detach"),
            mount.as_os_str(),
            OsStr::new("-quiet"),
        ],
    );
    let staged = staged?;

    match verify(&staged, app) {
        Ok(()) => Ok(staged),
        // A copy that does not verify is not left lying next to the app it
        // failed to become.
        Err(err) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(err)
        }
    }
}

/// Copy the mounted bundle into the staging directory.
///
/// `ditto` rather than `cp -R`: it carries the extended attributes and symlinks
/// a bundle is made of, and a signature does not survive a copy that drops
/// them. The name is read off the image rather than assumed, so a renamed
/// install is still updated by the image it came from.
fn copy_out(mount: &Path, staging: &Path) -> Result<PathBuf> {
    let bundle = std::fs::read_dir(mount)?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "app"))
        .context("the image holds no app")?;
    let staged = staging.join(bundle.file_name().context("the app has no name")?);
    run("/usr/bin/ditto", [bundle.as_os_str(), staged.as_os_str()])?;
    Ok(staged)
}

/// The assessment Gatekeeper will not make, because nothing marked this
/// download as having come from anywhere — see the module note.
fn verify(staged: &Path, running: &Path) -> Result<()> {
    run(
        "/usr/bin/codesign",
        [
            OsStr::new("--verify"),
            OsStr::new("--strict"),
            staged.as_os_str(),
        ],
    )
    .context("the release does not match its own signature")?;
    run(
        "/usr/sbin/spctl",
        [
            OsStr::new("--assess"),
            OsStr::new("--type"),
            OsStr::new("exec"),
            staged.as_os_str(),
        ],
    )
    .context("the release is not notarized")?;
    // Whoever signed the copy that is running is the only publisher this app
    // will take a replacement from. No certificate is named anywhere in the
    // source — an ad-hoc local build has no team at all, and refuses every
    // release, which is the safe side of that trade.
    if team(staged)? != team(running)? {
        bail!("the release is signed by another developer than this copy");
    }
    Ok(())
}

/// The Apple team a bundle is signed by, or `None` for one signed ad-hoc.
fn team(bundle: &Path) -> Result<Option<String>> {
    let out = run(
        "/usr/bin/codesign",
        [
            OsStr::new("-d"),
            OsStr::new("--verbose=4"),
            bundle.as_os_str(),
        ],
    )?;
    // `codesign -d` reports on stderr, one `key=value` to a line.
    Ok(String::from_utf8_lossy(&out.stderr)
        .lines()
        .find_map(|line| line.strip_prefix("TeamIdentifier="))
        .filter(|team| *team != "not set")
        .map(str::to_owned))
}

/// Swap the staged bundle in once this process is gone, and open it.
pub(super) fn swap_on_exit(app: &Path, staged: &Path) -> Result<()> {
    super::swap_on_exit(app, staged, r#"open "$app""#)
}
