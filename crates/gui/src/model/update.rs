//! Restart to update: which release is out, the copy of it staged beside this
//! one, and the swap that happens on the way out.
//!
//! Three things are kept apart on purpose.
//!
//! * **Looking** is the app's own business and happens quietly — on launch and
//!   every hour after, unless `auto_update` says no.
//! * **Fetching** follows a find, also quietly. A release is ~50MB and the
//!   machine is idle; nothing is asked of anybody to have it ready.
//! * **Swapping** is nobody's business but the person using the app. It is one
//!   click, and it is the last thing that happens before the process exits.
//!
//! The swap is deliberately *not* done while the app runs, which is where this
//! parts company with Zed: replacing the bundle underneath a live process
//! leaves it with resources that no longer match its own code, and a crash
//! after that lands the Dock on a version nobody asked for. Here the staged
//! bundle waits, and the rename happens in a script that starts by waiting for
//! this process to be gone. A person who never clicks restart carries an unused
//! directory and nothing else.
//!
//! What makes any of this cheap is that quitting is already survivable:
//! `workspace::save` writes on every change and sessions are on disk and
//! resumable, so a restart here is the same door as ⌘Q — which is the argument
//! [`crate::view::menubar`] already makes for closing the window.
//!
//! What a release is and how it is swapped in is per platform: a dmg on macOS
//! (`update/macos.rs`), the tarball on Linux (`update/linux.rs`), the installer
//! on Windows (`update/windows.rs`).

use crate::model::settings;
#[cfg(unix)]
use anyhow::bail;
use anyhow::{Context as _, Result};
use bezel::gpui::{App, AppContext as _, Context, Entity, Global, SharedString, Task};
use serde::Deserialize;
#[cfg(unix)]
use std::{
    ffi::OsStr,
    process::{Command, Output},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

/// Every other platform: no release is cut for it.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use anyhow::{Result, bail};
    use bezel::gpui::App;
    use std::path::{Path, PathBuf};

    pub(super) const SUPPORTED: bool = false;
    pub(super) const SUFFIX: &str = "";
    pub(super) fn remote(_: &str) -> String {
        String::new()
    }
    pub(super) fn bundle(_: &App) -> Option<PathBuf> {
        None
    }
    pub(super) fn stage(_: &str, _: &Path) -> Result<PathBuf> {
        bail!("no release is cut for this platform")
    }
    pub(super) fn swap_on_exit(_: &Path, _: &Path) -> Result<()> {
        bail!("no release is cut for this platform")
    }
}

/// The feed: the same history the changelog page renders, as a file on the CDN
/// — see `www/src/routes/changelog.json/+server.js`. Built out of the homepage
/// so the host is named in `Cargo.toml` and nowhere else.
pub const FEED: &str = concat!(env!("CARGO_PKG_HOMEPAGE"), "/changelog.json");

/// This build, and where its releases are published.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = env!("CARGO_PKG_REPOSITORY");

/// What every image is named after, ahead of the version it carries.
const IMAGE: &str = "cydonia-";

/// And what a staged copy's directory is named after, beside the app it is
/// waiting to become. Dot-prefixed so a file manager keeps it out of the way.
const STAGING: &str = ".cydonia-update-";

/// How long after launch the first check waits. Behind the window and the
/// first paint, and no longer: opening the app is when being out of date is
/// worth hearing about, and the read itself is a few kilobytes on a background
/// thread.
const FIRST: Duration = Duration::from_secs(3);

/// And the gap between the ones after it.
const EVERY: Duration = Duration::from_secs(60 * 60);

/// The feed is a few kilobytes and nothing waits on it, so it gives up early.
const FEED_TIMEOUT: Duration = Duration::from_secs(10);

/// The image is not, and a slow line is not a failure — only a line that never
/// opens is, which is what this bounds.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Where the app has got to. Every state but [`Status::Ready`] is something the
/// app is doing or has finished doing; that one is a thing to press.
#[derive(Clone)]
pub enum Status {
    /// Nothing asked yet — or asked, failed, and not worth saying so. A check
    /// nobody started cannot report a network that is not there.
    Idle,
    Checking,
    /// The feed was read and this copy is the newest there is.
    Current,
    /// A release is out, and this build is not one it can stand in for — a
    /// working copy, a `cargo install` binary, an architecture no image is cut
    /// for. News rather than a thing to press: the site is where it is picked
    /// up, and [`supported`] is what tells the two apart.
    Available(SharedString),
    Downloading(SharedString),
    /// A verified bundle is staged. The version is what to say; the path is
    /// what the swap moves, and the two travel together so that a `Ready`
    /// with nothing behind it cannot be built.
    Ready {
        version: SharedString,
        staged: PathBuf,
    },
    Failed(SharedString),
}

/// One release, off the feed. Only the version is read — the image's address is
/// derived from it, the same way the download button on the site derives it — so
/// a release publishes nothing here beyond the entry it already writes.
#[derive(Deserialize)]
struct Entry {
    version: String,
}

pub struct Updater {
    status: Status,
    /// The bundle this process runs from, when it runs from one this can
    /// replace. `None` is every other build — a bare `cargo install` binary, a
    /// `cargo run` during development, an architecture with no image — and it
    /// is what [`of`] reads to decide whether the app shows any of this.
    app: Option<PathBuf>,
    /// The poll, held rather than detached: dropping it stops it, which is what
    /// switching `auto_update` off does.
    poll: Option<Task<()>>,
    /// Whether the notifier is being looked at rather than meant — the
    /// Developer section's switch. Held here because the thing it has to reach
    /// is in another window; kept out of [`Status`] because a pretended release
    /// has no bundle behind it, and a state that says it has would be one
    /// [`Updater::restart`] could act on.
    preview: bool,
}

/// The one updater, reached from the menu bar and from settings.
struct Handle(Entity<Updater>);

impl Global for Handle {}

/// Install it, beside the other `init`s. `auto` is `settings.auto_update`.
pub fn init(auto: bool, cx: &mut App) {
    let app = platform::SUPPORTED.then(|| platform::bundle(cx)).flatten();
    if let Some(app) = app.as_deref() {
        sweep(app);
    }
    let updater = cx.new(|cx| {
        let mut this = Updater {
            status: Status::Idle,
            app,
            poll: None,
            preview: false,
        };
        if auto {
            this.poll(cx);
        }
        this
    });
    cx.set_global(Handle(updater));
}

/// The updater. There is one in every build, because the Developer section can
/// put its notifier on screen in a build no release could ever replace — which
/// is most of them, `cargo run` included.
pub fn of(cx: &App) -> Option<Entity<Updater>> {
    Some(cx.try_global::<Handle>()?.0.clone())
}

/// Whether a release could actually replace this build: a bundle, on the
/// architecture an image is cut for. What the menu item hangs off — a build
/// this is false for gets no item, rather than one that would decline.
///
/// Settings shows the Updates box either way and reads this for what to say in
/// it: looking is worth doing in any build, and a release that has to be
/// downloaded by hand is still one to hear about.
pub fn supported(cx: &App) -> bool {
    cx.try_global::<Handle>()
        .is_some_and(|handle| handle.0.read(cx).app.is_some())
}

impl Updater {
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// The version a restart would put in, for whatever says so outside of
    /// settings — see [`crate::view::sidebar`].
    ///
    /// The preview answers here and nowhere else, so the one thing it can do is
    /// put the notifier on screen. Pressing it while it is pretending does
    /// nothing at all: [`Self::restart`] reads the status, which a preview
    /// never touches.
    pub fn ready(&self) -> Option<SharedString> {
        if self.preview {
            return Some(pretend());
        }
        match &self.status {
            Status::Ready { version, .. } => Some(version.clone()),
            _ => None,
        }
    }

    pub fn previewing(&self) -> bool {
        self.preview
    }

    /// Show the notifier for a release that has not happened. In memory only —
    /// a switch for looking at something is not a preference, and a relaunch is
    /// the right way to put it down.
    pub fn set_preview(&mut self, on: bool, cx: &mut Context<Self>) {
        self.preview = on;
        cx.notify();
    }

    /// Read the feed, and fetch what it names if that is newer than this.
    ///
    /// `manual` is whether somebody asked. A check nobody asked for keeps its
    /// failures to itself — a laptop off the network would otherwise leave
    /// "could not check" sitting in settings for the rest of the session — and
    /// the menu item's check says what went wrong.
    pub fn check(&mut self, manual: bool, cx: &mut Context<Self>) {
        // Already looking, already fetching, or already holding one: none of
        // those are improved by a second pass.
        if matches!(
            self.status,
            Status::Checking | Status::Downloading(_) | Status::Ready { .. }
        ) {
            return;
        }
        // `None` is a build nothing can be staged beside. It still looks — what
        // release is out is worth knowing in any build, and settings says so —
        // it just stops at knowing.
        let app = self.app.clone();
        self.status = Status::Checking;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async { latest() }).await;
            let release = match found {
                Ok(Some(release)) => release,
                Ok(None) => {
                    let _ = this.update(cx, |this, cx| this.settle(Status::Current, cx));
                    return;
                }
                Err(err) => {
                    let _ = this.update(cx, |this, cx| this.settle(failure(err, manual), cx));
                    return;
                }
            };
            let version = SharedString::from(release.clone());
            let Some(app) = app else {
                let _ = this.update(cx, |this, cx| this.settle(Status::Available(version), cx));
                return;
            };
            if this
                .update(cx, |this, cx| {
                    this.settle(Status::Downloading(version.clone()), cx)
                })
                .is_err()
            {
                return;
            }
            let staged = cx
                .background_executor()
                .spawn(async move { platform::stage(&release, &app) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let status = match staged {
                    Ok(staged) => Status::Ready { version, staged },
                    Err(err) => failure(err, manual),
                };
                this.settle(status, cx);
            });
        })
        .detach();
    }

    /// Hand the swap to a script that outlives this process, and go.
    ///
    /// Nothing is asked about what is running: the sessions are on disk and
    /// resume, so this is ⌘Q with one rename in front of it.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        // Scoped, so the borrow of what is staged is over before a failure is
        // written back onto it.
        let handed = {
            let (Status::Ready { staged, .. }, Some(app)) = (&self.status, &self.app) else {
                return;
            };
            platform::swap_on_exit(app, staged)
        };
        match handed {
            // Not `cx.restart()`: that script re-opens the bundle and nothing
            // else, and the swap has to happen between the two.
            Ok(()) => cx.quit(),
            Err(err) => self.settle(Status::Failed(format!("{err:#}").into()), cx),
        }
    }

    /// Start or stop looking on our own. The file is written by
    /// [`crate::model::workspace::Workspace::set_auto_update`]; this is the
    /// part that runs. A release already staged stays staged — switching the
    /// looking off is not a reason to throw away what was found.
    pub fn set_auto(&mut self, on: bool, cx: &mut Context<Self>) {
        match (on, self.poll.is_some()) {
            (true, false) => self.poll(cx),
            (false, true) => self.poll = None,
            _ => {}
        }
    }

    /// The loop: a check after launch, then one every hour until there is
    /// a release in hand — or, in a build that can hold none, until the feed
    /// names one. Either way that is the last thing there is to find.
    fn poll(&mut self, cx: &mut Context<Self>) {
        self.poll = Some(cx.spawn(async move |this, cx| {
            let mut delay = FIRST;
            loop {
                cx.background_executor().timer(delay).await;
                delay = EVERY;
                let looking = this.update(cx, |this, cx| {
                    this.check(false, cx);
                    !matches!(this.status, Status::Ready { .. } | Status::Available(_))
                });
                if !matches!(looking, Ok(true)) {
                    return;
                }
            }
        }));
    }

    fn settle(&mut self, status: Status, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }
}

/// The file a version is kept as on disk, whatever the release calls it:
/// always named after the version, so [`version_of`] can read it back.
pub fn asset(version: &str) -> String {
    format!("{IMAGE}{version}{}", platform::SUFFIX)
}

/// And the version back out of that name, for a file found on disk rather than
/// asked for. `None` is a name this did not write — a half-finished download
/// among them — which [`sweep`] reads as "not worth keeping".
pub fn version_of(image: &Path) -> Option<String> {
    let name = image.file_name()?.to_str()?;
    Some(
        name.strip_prefix(IMAGE)?
            .strip_suffix(platform::SUFFIX)?
            .to_owned(),
    )
    .filter(|version| !version.is_empty())
}

/// And where that release is published: under the tag named after the
/// version, which is the whole reason the feed carries no addresses.
pub fn url(version: &str) -> String {
    format!(
        "{REPO}/releases/download/v{version}/{}",
        platform::remote(version)
    )
}

/// The version the feed offers, if it is ahead of this build.
fn latest() -> Result<Option<String>> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(FEED_TIMEOUT))
        .build()
        .new_agent();
    let body = agent
        .get(FEED)
        .call()
        .context("the release feed could not be read")?
        .body_mut()
        .read_to_string()?;
    let version = head(&body)?;
    Ok(newer(&version, VERSION).then_some(version))
}

/// The version at the head of a feed.
///
/// Its own function so that the shape this build depends on can be checked
/// against the file that actually ships — `changelog.json` is written by hand,
/// and nothing else here would notice the day its shape moved.
pub fn head(feed: &str) -> Result<String> {
    let entries: Vec<Entry> =
        serde_json::from_str(feed).context("the release feed is not the shape this build reads")?;
    // Newest first, which is what the page rendering this file reads too.
    let newest = entries
        .into_iter()
        .next()
        .context("the release feed is empty")?;
    Ok(newest.version)
}

/// Whether `offered` is a release to move to from `running`.
///
/// Three numbers and nothing else. A version either side cannot be read this
/// way — a pre-release tag, a build suffix — is not one to offer: the answer to
/// a string this cannot order is to leave the app where it is.
pub fn newer(offered: &str, running: &str) -> bool {
    match (numbers(offered), numbers(running)) {
        (Some(offered), Some(running)) => offered > running,
        _ => false,
    }
}

fn numbers(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.split('.');
    let mut out = [0; 3];
    for slot in &mut out {
        *slot = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// What the preview calls itself: this build with its patch moved on, which is
/// what the release after any given one nearly always is. A representative
/// string rather than a placeholder, because the width of it is half of what
/// there is to look at.
fn pretend() -> SharedString {
    match numbers(VERSION) {
        Some([major, minor, patch]) => format!("{major}.{minor}.{}", patch + 1).into(),
        None => VERSION.into(),
    }
}

/// The image on disk, fetched if it is not there yet.
///
/// Written under a name of its own and renamed into place, so an interrupted
/// download cannot be mistaken for a finished one on the next launch.
fn download(version: &str) -> Result<PathBuf> {
    let dir = downloads()?;
    std::fs::create_dir_all(&dir)?;
    let image = dir.join(asset(version));
    if image.is_file() {
        return Ok(image);
    }
    let part = image.with_extension("part");
    let fetched = (|| -> Result<()> {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .build()
            .new_agent();
        let mut body = agent.get(url(version)).call()?;
        let mut file = std::fs::File::create(&part)?;
        // Streamed rather than read whole: `read_to_vec` stops at ten
        // megabytes, and holding fifty in memory to write them out is no
        // better than not.
        std::io::copy(&mut body.body_mut().as_reader(), &mut file)?;
        file.sync_all()?;
        Ok(())
    })();
    if fetched.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    fetched.with_context(|| format!("cydonia {version} could not be downloaded"))?;
    std::fs::rename(&part, &image)?;
    Ok(image)
}

/// Drop everything a release left behind that this build is already at or past:
/// the image it was fetched as, and the directory it waited in.
///
/// What a swap just installed is the first of those — the app now *is* the
/// version those are named for — so a release clears its own fifty megabytes on
/// the launch after it lands, and an abandoned download or a restart that was
/// never asked for goes the same way. Best effort throughout: a file that will
/// not delete is not a reason to fail a launch.
fn sweep(app: &Path) {
    if let Ok(dir) = downloads()
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        for path in entries.flatten().map(|entry| entry.path()) {
            // Every file here is an image or the remains of one — see
            // [`downloads`] — so a name this cannot read a version out of is
            // something interrupted rather than something to keep.
            if version_of(&path).is_none_or(|version| !newer(&version, VERSION)) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    // Beside the app is somebody else's directory, so nothing there is touched
    // unless we named it.
    if let Some(parent) = app.parent()
        && let Ok(entries) = std::fs::read_dir(parent)
    {
        for path in entries.flatten().map(|entry| entry.path()) {
            let ours = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(spent);
            if ours {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

/// Whether a name beside the app is a staging directory of ours that has
/// nothing left to give — the version it holds is one this build is at or past.
///
/// The predicate a `remove_dir_all` next to `/Applications` runs on, which is
/// the reason it is out here where it can be read and tested on its own.
pub fn spent(name: &str) -> bool {
    name.strip_prefix(STAGING)
        .is_some_and(|version| !newer(version, VERSION))
}

/// The throwaway corner of the config directory [`crate::agent::cache_dir`]
/// keeps the fetched catalog in. Two things of ours live under it: the images,
/// and the point an image is mounted at while it is read.
fn cache() -> Result<PathBuf> {
    Ok(settings::dir()?.join("cache"))
}

/// Where an image waits. Nothing but images is kept here — [`sweep`] deletes
/// what it finds and does not recognise.
fn downloads() -> Result<PathBuf> {
    Ok(cache()?.join("updates"))
}

/// Wait for this process to be gone, put the staged copy where the running one
/// is, and run `launch`.
///
/// `$0` is the pid, `$1` the app and `$2` the staged copy — the same shape
/// gpui's own restart script uses. The old copy is moved aside rather than
/// deleted, so a rename that fails part way can be undone; `launch` runs
/// whatever the outcome, because the one thing this must never do is leave
/// somebody with no app at all.
#[cfg(unix)]
const SWAP: &str = r#"
    while kill -0 $0 2> /dev/null; do
        sleep 0.1
    done
    app="$1"
    staged="$2"
    old="$app.old"
    rm -rf "$old"
    if mv "$app" "$old"; then
        if mv "$staged" "$app"; then
            rm -rf "$old"
            rmdir "$(dirname "$staged")" 2> /dev/null
        else
            mv "$old" "$app"
        fi
    fi
"#;

#[cfg(unix)]
fn swap_on_exit(app: &Path, staged: &Path, launch: &str) -> Result<()> {
    use std::os::unix::process::CommandExt as _;
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(format!("{SWAP}    {launch}\n"))
        .arg(std::process::id().to_string())
        .arg(app)
        .arg(staged)
        // A process group of its own, so whatever ends this process does not
        // take the swap with it. gpui's own `restart` does the same.
        .process_group(0);
    command.spawn().context("the update script did not start")?;
    Ok(())
}

/// Run a tool, and turn a non-zero exit into the error it printed.
#[cfg(unix)]
fn run<'a>(program: &str, args: impl IntoIterator<Item = &'a OsStr>) -> Result<Output> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("{program} could not be run"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let line = stderr.lines().find(|line| !line.trim().is_empty());
        bail!("{program}: {}", line.unwrap_or("failed").trim());
    }
    Ok(out)
}

/// What a failure comes out as: something to read when a person asked, and
/// nothing at all when they did not.
fn failure(err: anyhow::Error, manual: bool) -> Status {
    if manual {
        Status::Failed(format!("{err:#}").into())
    } else {
        Status::Idle
    }
}
