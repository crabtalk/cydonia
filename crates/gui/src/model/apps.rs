//! Mini-apps: crates in a project's `apps/` built against `cydonia-guest`,
//! each a [`Program`], and each open view holding one running instance of it
//! through [`Runner`].

use anyhow::{Context as _, Result, bail};
use artifact::app::Source;
use bezel::gpui::{App, AppContext as _, Context, Entity, Task};
use std::{ops::Range, process, sync::Arc};

pub use guest::{
    manifest::Manifest,
    wire::{Answer, Call, CallId, Event, Node},
};

/// The target apps are built for.
const TARGET: &str = "wasm32-unknown-unknown";

/// What this build runs apps on.
#[cfg(feature = "desktop")]
pub type Runtime = wasmtime::Wasmtime;
#[cfg(not(feature = "desktop"))]
pub type Runtime = none::Absent;

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

/// An app a project holds: its directory's name, which never changes, and the
/// program, which every view drawing any of it observes.
pub struct HeldApp {
    pub name: String,
    pub program: Entity<Program>,
}

/// One app's source, and what building it gave.
pub struct Program {
    pub source: Source,
    pub number: Option<u64>,
    /// The last good build's manifest.
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
    pub fn new(source: Source, number: Option<u64>) -> Self {
        Self {
            manifest: source.manifest(),
            source,
            number,
            wasm: None,
            build: Build::Idle,
            wanted: false,
            _building: None,
        }
    }

    /// What the app is listed as: its manifest's name, or its directory's
    /// before a build has given it one.
    pub fn label(&self) -> String {
        self.manifest
            .as_ref()
            .map(|manifest| manifest.name.clone())
            .unwrap_or_else(|| self.source.name.clone())
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
        if self.wanted {
            self.rebuild(cx);
        }
    }

    /// Build now. A build under way is dropped for this one.
    pub fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.build = Build::Running;
        let source = self.source.clone();
        self._building = Some(cx.spawn(async move |this, cx| {
            let built = cx
                .background_executor()
                .spawn(async move { Program::compile(&source) })
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
    pub fn new(source: Source, number: Option<u64>, cx: &mut App) -> Self {
        Self {
            name: source.name.clone(),
            program: cx.new(|_| Program::new(source, number)),
        }
    }

    pub fn read<'a>(&self, cx: &'a App) -> &'a Program {
        self.program.read(cx)
    }
}

impl Program {
    /// Build `source` with cargo, keep the `.wasm` where [`Source::built`] says,
    /// and answer with it and its manifest. Blocks until cargo is done.
    pub fn compile(source: &Source) -> Result<(Vec<u8>, Manifest)> {
        let output = process::Command::new("cargo")
            .args(["build", "--release", "--quiet", "--message-format=json"])
            .args(["--target", TARGET])
            .arg("--manifest-path")
            .arg(source.manifest_path())
            .arg("--target-dir")
            .arg(source.target())
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
        artifact::fs::save(&source.built(), &wasm)?;
        Ok((wasm, manifest))
    }
}
