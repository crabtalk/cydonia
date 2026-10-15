//! Cargo building an app's crate. Cargo leads a process group of its own (a
//! job object on Windows), so stopping a build kills cargo and every process
//! it started.

use super::{Manifest, TARGET};
use anyhow::{Context as _, Result, bail};
use artifact::app::Crate;
use process_wrap::std::{ChildWrapper, CommandWrap};
use std::{
    io::Read as _,
    process::{ChildStderr, ChildStdout, Stdio},
    sync::{Arc, Mutex},
};

/// Cargo's process while it runs; `None` once [`Cargo::finish`] has waited on
/// it.
type Child = Arc<Mutex<Option<Box<dyn ChildWrapper>>>>;

/// A build under way.
pub struct Cargo {
    krate: Crate,
    child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
}

/// Kills the build it came from when dropped, unless that build has finished.
pub struct Stop(Child);

impl Cargo {
    /// Start building `krate`.
    pub fn start(krate: &Crate) -> Result<Self> {
        let mut command = CommandWrap::with_new("cargo", |command| {
            command
                .args(["build", "--release", "--quiet", "--message-format=json"])
                .args(["--target", TARGET])
                .arg("--manifest-path")
                .arg(krate.manifest_path())
                .arg("--target-dir")
                .arg(&krate.target)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        });
        #[cfg(unix)]
        command.wrap(process_wrap::std::ProcessGroup::leader());
        #[cfg(windows)]
        command.wrap(process_wrap::std::JobObject);
        let mut child = match command.spawn() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                bail!("Building an app takes cargo, and there is no cargo on this machine's PATH")
            }
            spawned => spawned.context("cargo did not start")?,
        };
        let stdout = child.stdout().take().context("cargo's output")?;
        let stderr = child.stderr().take().context("cargo's errors")?;
        Ok(Self {
            krate: krate.clone(),
            child: Arc::new(Mutex::new(Some(child))),
            stdout,
            stderr,
        })
    }

    pub fn stop(&self) -> Stop {
        Stop(self.child.clone())
    }

    /// Wait for cargo, keep the `.wasm` where [`Crate::built`] says, and
    /// answer with it and its manifest. Blocks until cargo is done or
    /// stopped.
    pub fn finish(mut self) -> Result<(Vec<u8>, Manifest)> {
        let mut stderr = self.stderr;
        let errors = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        });
        let mut stdout = String::new();
        let _ = self.stdout.read_to_string(&mut stdout);
        let stderr = errors.join().unwrap_or_default();
        let child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("cargo's process was poisoned"))?
            .take();
        let status = child.context("the build was stopped")?.wait()?;
        let messages = stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok());
        if !status.success() {
            let rendered: Vec<String> = messages
                .filter(|message| message["reason"] == "compiler-message")
                .filter_map(|message| Some(message["message"]["rendered"].as_str()?.to_owned()))
                .collect();
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
        artifact::fs::save(&self.krate.built, &wasm)?;
        Ok((wasm, manifest))
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        if let Ok(mut child) = self.0.lock()
            && let Some(child) = child.as_mut()
        {
            let _ = child.start_kill();
        }
    }
}
