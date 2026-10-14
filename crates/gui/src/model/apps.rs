//! Mini-apps: crates built against `cydonia-guest`, in a project's `apps/` or
//! loaded from a folder, and apps installed from a registry, each a
//! [`Program`], and each open view holding one running instance of it through
//! [`Runner`].

use anyhow::{Context as _, Result, bail};
use artifact::app::{Crate, Installed};
use bezel::gpui::{App, AppContext as _, Context, Entity, Task};
use std::{ops::Range, process, sync::Arc};

pub use artifact::app::Origin;
pub use guest::{
    manifest::Manifest,
    wire::{Answer, Call, CallId, Event, Node, Request, Response},
};

/// The target apps are built for.
const TARGET: &str = "wasm32-unknown-unknown";

/// What this build runs apps on.
#[cfg(feature = "desktop")]
pub type Runtime = wasmtime::Wasmtime;
#[cfg(not(feature = "desktop"))]
pub type Runtime = none::Absent;

/// What this build makes an app's HTTP requests with.
#[cfg(feature = "desktop")]
pub type Http = http::Ureq;
#[cfg(not(feature = "desktop"))]
pub type Http = none::Offline;

pub mod oci;

#[cfg(feature = "desktop")]
mod http;
#[cfg(feature = "desktop")]
mod wasmtime;

#[cfg(not(feature = "desktop"))]
mod none;

/// What an app asked of Cydonia during a call, for Cydonia to do once the
/// call has returned.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Put `value` in the input `key`.
    SetInput { key: String, value: String },
    /// Do `call`, and deliver its answer as an [`Event::Answer`] numbered `id`.
    Call { id: CallId, call: Call },
}

/// HTTP for apps. Blocks until the response is read.
pub trait Fetcher {
    fn fetch(request: &Request) -> Result<Response>;
}

/// One running instance of an app. A call that fails leaves the instance
/// unusable: the error is the app's last word.
pub trait Runner: Sized + Send + 'static {
    /// Compile `wasm`, instantiate it and run its `init`.
    fn start(wasm: &[u8]) -> Result<(Self, Vec<Command>)>;

    /// Deliver `event`.
    fn update(&mut self, event: &Event) -> Result<Vec<Command>>;

    /// What the app draws now.
    fn view(&mut self) -> Result<Node>;

    /// The rows `range` of the list `list` the last view drew.
    fn rows(&mut self, list: &str, range: Range<u32>) -> Result<Vec<Node>>;
}

/// An app a project holds: what the project lists it by, which never changes,
/// and the program, which every view drawing any of it observes.
pub struct HeldApp {
    /// [`Origin::name`].
    pub name: String,
    pub program: Entity<Program>,
}

/// One app, and the build its views run: what cargo made of a crate, or an
/// installed app's pinned `.wasm`.
pub struct Program {
    pub origin: Origin,
    pub number: Option<u64>,
    /// The last good build's manifest, or before a crate's first build, what
    /// its `Cargo.toml` declares.
    pub manifest: Option<Manifest>,
    /// The last good build, which the app's views run.
    pub wasm: Option<Arc<[u8]>>,
    pub build: Build,
    /// Whether a view has asked for the app since it was listed. A change to
    /// its source rebuilds only an app that was.
    wanted: bool,
    _building: Option<Task<()>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Build {
    Idle,
    Running,
    Failed(String),
}

impl Program {
    pub fn new(origin: Origin, number: Option<u64>) -> Self {
        Self {
            manifest: origin.manifest(),
            origin,
            number,
            wasm: None,
            build: Build::Idle,
            wanted: false,
            _building: None,
        }
    }

    /// What the app is listed as: its manifest's name, or before a build has
    /// given it one, what the project lists it by.
    pub fn label(&self) -> String {
        self.manifest
            .as_ref()
            .map(|manifest| manifest.name.clone())
            .unwrap_or_else(|| self.origin.name().into_owned())
    }

    /// Whether Cydonia builds it from a crate on this machine.
    pub fn local(&self) -> bool {
        self.origin.local().is_some()
    }

    /// Take a re-read of where the app comes from: an installed app pulled
    /// again, or let run.
    pub fn adopt(&mut self, origin: Origin, cx: &mut Context<Self>) {
        if self.origin == origin {
            return;
        }
        self.origin = origin;
        if self.wanted {
            self.rebuild(cx);
        }
        cx.notify();
    }

    /// Let an installed app's pinned digest run in its project.
    pub fn allow(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let Origin::Installed { project, app } = &mut self.origin else {
            return Ok(());
        };
        app.allowed = Some(app.digest.clone());
        app.save(project)?;
        cx.notify();
        Ok(())
    }

    /// Build the app for a view about to run it, once.
    pub fn want(&mut self, cx: &mut Context<Self>) {
        if !self.wanted {
            self.wanted = true;
            self.rebuild(cx);
        }
    }

    /// Its source moved: build it again if a view has asked for it.
    pub fn changed(&mut self, cx: &mut Context<Self>) {
        if self.wanted && self.local() {
            self.rebuild(cx);
        }
    }

    /// Build a crate now, or read an installed app's `.wasm`. A build under
    /// way is dropped for this one.
    pub fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.build = Build::Running;
        let origin = self.origin.clone();
        self._building = Some(cx.spawn(async move |this, cx| {
            let built = cx
                .background_executor()
                .spawn(async move {
                    match origin {
                        Origin::Source(source) => Program::compile(&source.krate()),
                        Origin::Loaded { project, app } => Program::compile(&app.krate(&project)),
                        Origin::Installed { app, .. } => Program::load(&app),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match built {
                    Ok((wasm, manifest)) => {
                        this.manifest = Some(manifest);
                        this.wasm = Some(wasm.into());
                        this.build = Build::Idle;
                    }
                    Err(error) => this.build = Build::Failed(format!("{error:#}")),
                }
                this._building = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl HeldApp {
    pub fn new(origin: Origin, number: Option<u64>, cx: &mut App) -> Self {
        Self {
            name: origin.name().into_owned(),
            program: cx.new(|_| Program::new(origin, number)),
        }
    }

    pub fn read<'a>(&self, cx: &'a App) -> &'a Program {
        self.program.read(cx)
    }
}

impl Program {
    /// An installed app's pinned `.wasm` and its manifest, checked against
    /// the digest it is pinned to.
    pub fn load(app: &Installed) -> Result<(Vec<u8>, Manifest)> {
        use sha2::{Digest as _, Sha256};
        let path = app.wasm()?;
        let wasm = std::fs::read(&path)
            .with_context(|| format!("{} is not installed on this machine", app.reference))?;
        let found = format!("sha256:{:x}", Sha256::digest(&wasm));
        anyhow::ensure!(
            found == app.digest,
            "{} is not the {} it was pinned to",
            path.display(),
            app.digest
        );
        let manifest = Manifest::read(&wasm)
            .map_err(anyhow::Error::msg)?
            .context("The installed app has no manifest")?;
        Ok((wasm, manifest))
    }

    /// Build `krate` with cargo, keep the `.wasm` where [`Crate::built`] says,
    /// and answer with it and its manifest. Blocks until cargo is done.
    pub fn compile(krate: &Crate) -> Result<(Vec<u8>, Manifest)> {
        let output = process::Command::new("cargo")
            .args(["build", "--release", "--quiet", "--message-format=json"])
            .args(["--target", TARGET])
            .arg("--manifest-path")
            .arg(krate.manifest_path())
            .arg("--target-dir")
            .arg(&krate.target)
            .output();
        let output = match output {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                bail!("Building an app takes cargo, and there is no cargo on this machine's PATH")
            }
            output => output.context("cargo did not start")?,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let messages = stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok());
        if !output.status.success() {
            let rendered: Vec<String> = messages
                .filter(|message| message["reason"] == "compiler-message")
                .filter_map(|message| Some(message["message"]["rendered"].as_str()?.to_owned()))
                .collect();
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("{}{}", rendered.join(""), stderr.trim());
        }
        let built = messages
            .filter(|message| message["reason"] == "compiler-artifact")
            .flat_map(|message| {
                message["filenames"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|name| name.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .rfind(|name| name.ends_with(".wasm"))
            .context("cargo built no .wasm: the crate needs `crate-type = [\"cdylib\"]`")?;
        let wasm = std::fs::read(&built)?;
        let manifest = Manifest::read(&wasm).map_err(anyhow::Error::msg)?.context(
            "The app has no manifest: `cydonia_guest::app!` writes it, in the crate root",
        )?;
        artifact::fs::save(&krate.built, &wasm)?;
        Ok((wasm, manifest))
    }
}
