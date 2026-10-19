//! Mini-apps: crates built against `cydonia-guest` and loaded from a folder,
//! and apps installed from a registry, each a [`Program`], and each open view
//! holding one running instance of it through [`Runner`]. An app belongs to no
//! project.

use crate::data::{Calls as _, Data, Here, Home, Local as _};
use anyhow::{Context as _, Result};
use artifact::app::{Home as _, Installed};
use bezel::gpui::{App, AppContext as _, Context, Entity, Task};
use cargo::{Cargo, Stop};
use futures::{
    FutureExt as _,
    future::{self, BoxFuture},
};
use std::{ops::Range, sync::Arc};

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

pub mod cargo;
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

/// An app the workspace holds: its id, which never changes, and the program,
/// which every view drawing any of it observes.
pub struct HeldApp {
    pub id: String,
    pub program: Entity<Program>,
}

/// One app, and the build its views run: what cargo made of a crate, or an
/// installed app's pinned `.wasm`.
pub struct Program {
    pub origin: Origin,
    /// The last good build's manifest, or before a crate's first build, what
    /// its `Cargo.toml` declares.
    pub manifest: Option<Manifest>,
    /// The last good build, which the app's views run.
    pub wasm: Option<Arc<[u8]>>,
    pub build: Build,
    /// Whether a view has asked for the app since it was listed. A change to
    /// its source rebuilds only an app that was.
    wanted: bool,
    building: Option<Building>,
    /// The app's own tables, opened at its first call on them.
    data: Option<Data>,
}

/// A build under way. Dropping it stops it.
struct Building {
    _task: Task<()>,
    _stop: Option<Stop>,
}

/// What a build is: cargo on a crate, or reading an installed app's `.wasm`.
enum Work {
    Cargo(Cargo),
    Load(Installed),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Build {
    Idle,
    Running,
    Failed(String),
}

impl Program {
    /// The build under way stops when Cydonia quits.
    pub fn new(origin: Origin, cx: &mut Context<Self>) -> Self {
        cx.on_app_quit(|this: &mut Self, _| {
            this.stop();
            async {}
        })
        .detach();
        Self {
            manifest: origin.manifest(),
            origin,
            wasm: None,
            build: Build::Idle,
            wanted: false,
            building: None,
            data: None,
        }
    }

    /// What the app is listed as: its manifest's name, or its id.
    pub fn label(&self) -> String {
        self.manifest
            .as_ref()
            .map(|manifest| manifest.name.clone())
            .unwrap_or_else(|| self.origin.id().to_owned())
    }

    /// Whether Cydonia builds it from a crate on this machine.
    pub fn local(&self) -> bool {
        self.origin.local()
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

    /// Let an installed app's pinned digest run.
    pub fn allow(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let Origin::Installed(app) = &mut self.origin else {
            return Ok(());
        };
        app.allowed = Some(app.digest.clone());
        app.save()?;
        cx.notify();
        Ok(())
    }

    /// Stop the build under way, if one is.
    pub fn stop(&mut self) {
        self.building = None;
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
    /// way is stopped for this one.
    pub fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.building = None;
        let work = match Work::start(&self.origin) {
            Ok(work) => work,
            Err(error) => {
                self.build = Build::Failed(format!("{error:#}"));
                cx.notify();
                return;
            }
        };
        self.build = Build::Running;
        let stop = work.stop();
        let task = cx.spawn(async move |this, cx| {
            let built = cx
                .background_executor()
                .spawn(async move { work.run() })
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
                this.building = None;
                cx.notify();
            });
        });
        self.building = Some(Building {
            _task: task,
            _stop: stop,
        });
        cx.notify();
    }
}

impl Program {
    /// Answer the app's call on its own tables.
    pub fn table_call(&mut self, call: Call) -> BoxFuture<'static, Answer> {
        let answered = match self.tables() {
            Ok(data) => data.call(call),
            Err(error) => future::ready(Err(error)).boxed(),
        };
        answered
            .map(|answer| answer.unwrap_or_else(|error| Answer::Error(format!("{error:#}"))))
            .boxed()
    }

    /// The app's own tables, opened at the first call on them.
    fn tables(&mut self) -> Result<&mut Data> {
        let data = match self.data.take() {
            Some(data) => data,
            None => {
                let manifest = self.manifest.as_ref().context("The app has no manifest")?;
                Box::new(Here::open(&Home::App(manifest.home()?))?)
            }
        };
        Ok(self.data.insert(data))
    }
}

impl Work {
    fn start(origin: &Origin) -> Result<Self> {
        match origin {
            Origin::Loaded(app) => Cargo::start(&app.krate()?).map(Self::Cargo),
            Origin::Installed(app) => Ok(Self::Load(app.clone())),
        }
    }

    fn stop(&self) -> Option<Stop> {
        match self {
            Self::Cargo(cargo) => Some(cargo.stop()),
            Self::Load(_) => None,
        }
    }

    fn run(self) -> Result<(Vec<u8>, Manifest)> {
        match self {
            Self::Cargo(cargo) => cargo.finish(),
            Self::Load(app) => Program::load(&app),
        }
    }
}

impl HeldApp {
    pub fn new(origin: Origin, cx: &mut App) -> Self {
        Self {
            id: origin.id().to_owned(),
            program: cx.new(|cx| Program::new(origin, cx)),
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
}
