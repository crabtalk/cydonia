//! What cydonia hands Cupola for a remote host: cydonia-server, built from
//! the checkout this app was built from, ssh's prompts, answered by whoever
//! holds the [`Prompt`] receiver, and the services the app answers the
//! server with. [`Delegate::install`] puts Cupola's controller over the
//! saved hosts behind [`super::Remotes`].

use super::{Hosts, Remotes};
use crate::{agent::acp, model::settings};
use cupola::{
    Client,
    client::{
        controller::{Controller, Ssh},
        host::HostId,
        platform::{Os, Platform},
        session::{Heartbeat, Mode},
        ssh::{self, Control},
    },
};
use futures::{
    FutureExt as _,
    channel::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    future::BoxFuture,
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// The checkout this app was built from.
const CHECKOUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// Where server builds go, under the checkout.
const TARGET_DIR: &str = "target/cydonia-server";

/// The saved hosts, in the config directory.
const HOSTS: &str = "hosts.toml";

/// cydonia's [`ssh::Delegate`].
pub struct Delegate {
    prompts: UnboundedSender<Prompt>,
}

/// One of ssh's password or 2FA prompts, waiting for its answer. Dropped
/// unanswered, or answered `None`, it cancels the connection.
pub struct Prompt {
    pub text: String,
    pub answer: oneshot::Sender<Option<String>>,
}

/// How a server binary for one platform is built from a checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    checkout: PathBuf,
    target: Target,
}

/// One step of setting a build up: `run`, unless `unless` succeeds.
#[derive(Debug)]
pub struct Step {
    pub unless: Option<Command>,
    pub run: Command,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    /// This machine's own, with `cargo build`.
    Local,
    /// A static Linux binary for this triple, with `cargo zigbuild`.
    Musl(String),
}

impl Delegate {
    /// The delegate, and where its prompts arrive.
    pub fn new() -> (Self, UnboundedReceiver<Prompt>) {
        let (prompts, receiver) = mpsc::unbounded();
        (Self { prompts }, receiver)
    }

    /// Installs Cupola's controller over the hosts saved in the config
    /// directory's `hosts.toml`, connecting over ssh through a delegate, as
    /// the process's [`Remotes`]. Its sessions run on [`acp::runtime`].
    /// Answers where the delegate's prompts arrive.
    pub fn install() -> anyhow::Result<UnboundedReceiver<Prompt>> {
        let (delegate, prompts) = Self::new();
        let path = settings::dir()?.join(HOSTS);
        let (controller, runner) =
            Controller::open(path, Ssh::new(delegate), Heartbeat::default())?;
        Remotes::install(controller)?;
        acp::runtime().spawn(runner);
        Ok(prompts)
    }

    /// Where Cupola installs the server binary on a host whose home directory
    /// is `home`.
    pub fn program(home: &Path) -> PathBuf {
        home.join(ssh::ServerPath::new(server::NAME, server::VERSION).as_path())
    }
}

impl<D: ssh::Delegate> Hosts for Controller<Ssh<D>> {
    fn client(&self, host: HostId) -> anyhow::Result<Client> {
        Controller::client(self, host)
    }

    fn control(&self, host: HostId) -> Option<Control> {
        self.with_transport(host, |remote| remote.control().clone())
    }
}

impl ssh::Delegate for Delegate {
    fn name(&self) -> &str {
        server::NAME
    }

    fn version(&self) -> &str {
        server::VERSION
    }

    fn version_args(&self) -> Vec<OsString> {
        server::Command::Version.args()
    }

    fn proxy_args(&self, mode: Mode) -> Vec<OsString> {
        server::Command::Proxy(mode).args()
    }

    fn binary(&self, platform: Platform) -> BoxFuture<'static, anyhow::Result<PathBuf>> {
        let (sender, receiver) = oneshot::channel();
        let build = Build::new(Path::new(CHECKOUT), platform);
        std::thread::spawn(move || {
            let _ = sender.send(build.and_then(|build| build.run()));
        });
        receiver
            .map(|built| built.unwrap_or_else(|_| Err(anyhow::anyhow!("the build was lost"))))
            .boxed()
    }

    fn ask(&self, prompt: String) -> BoxFuture<'static, Option<String>> {
        let (answer, answered) = oneshot::channel();
        let sent = self.prompts.unbounded_send(Prompt {
            text: prompt,
            answer,
        });
        async move {
            sent.ok()?;
            answered.await.ok().flatten()
        }
        .boxed()
    }

    /// Answers [`server::mcp::Mcp`] with this app's MCP server.
    fn serve(&self, client: &Client) {
        server::mcp::Mcp::serve(client, crate::agent::serve::relayed);
    }
}

impl Build {
    /// A Mac of this machine's architecture gets this machine's build; Linux
    /// gets a musl build. Refuses any other platform.
    pub fn new(checkout: &Path, platform: Platform) -> anyhow::Result<Self> {
        let target = match platform.os {
            Os::Linux => Target::Musl(format!("{}-unknown-linux-musl", platform.arch.as_str())),
            Os::MacOs if Platform::local() == Some(platform) => Target::Local,
            Os::MacOs => anyhow::bail!("no cydonia-server for {platform} is built on this machine"),
        };
        Ok(Self {
            checkout: checkout.to_owned(),
            target,
        })
    }

    /// The command that builds the binary, run in the checkout.
    pub fn command(&self) -> Command {
        let mut command = Command::new("cargo");
        command.current_dir(&self.checkout);
        match &self.target {
            Target::Local => {
                command.arg("build");
            }
            Target::Musl(triple) => {
                command.args(["zigbuild", "--release", "--target", triple]);
            }
        }
        command
            .args(["-p", server::NAME, "--target-dir"])
            .arg(self.target_dir());
        command
    }

    /// Where [`Build::command`] leaves the binary.
    pub fn binary(&self) -> PathBuf {
        let built = match &self.target {
            Target::Local => self.target_dir().join("debug"),
            Target::Musl(triple) => self.target_dir().join(triple).join("release"),
        };
        built.join(server::NAME)
    }

    /// What must be in place before [`Build::command`] runs: for a musl
    /// build, cargo-zigbuild and the triple's standard library.
    pub fn setup(&self) -> Vec<Step> {
        let Target::Musl(triple) = &self.target else {
            return Vec::new();
        };
        let mut zigbuild = Command::new("cargo");
        zigbuild.args(["zigbuild", "--version"]);
        let mut install = Command::new("cargo");
        install.args(["install", "--locked", "cargo-zigbuild"]);
        let mut add = Command::new("rustup");
        add.args(["target", "add", triple]);
        vec![
            Step {
                unless: Some(zigbuild),
                run: install,
            },
            Step {
                unless: None,
                run: add,
            },
        ]
    }

    /// Sets up and builds, blocking until the binary is there.
    fn run(self) -> anyhow::Result<PathBuf> {
        for mut step in self.setup() {
            let done = step
                .unless
                .as_mut()
                .is_some_and(|check| check.output().is_ok_and(|output| output.status.success()));
            if !done {
                Self::succeed(&mut step.run)?;
            }
        }
        Self::succeed(&mut self.command())?;
        Ok(self.binary())
    }

    fn target_dir(&self) -> PathBuf {
        self.checkout.join(TARGET_DIR)
    }

    fn succeed(command: &mut Command) -> anyhow::Result<Output> {
        let output = command.output()?;
        anyhow::ensure!(
            output.status.success(),
            "{command:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(output)
    }
}
