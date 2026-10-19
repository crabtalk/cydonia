//! cydonia-server: what a cydonia client reaches on another machine, a Cupola
//! server answering cydonia's services. It has no GPUI dependency.
//!
//! The binary is run as one [`Command`]: ssh runs a proxy, the proxy starts
//! or reaches the daemon, and the daemon serves [`Services`]. An agent on the
//! machine runs it as an MCP server, which joins the daemon.

use std::ffi::OsString;
#[cfg(unix)]
use std::{io, path::PathBuf, process::ExitCode};

use crate::mcp::Binding;
use agents::Agents;
use artifact::{project::service::Artifact, table::service::Tables};
use cupola::{Server, client::session::Mode, exec::Exec, fs::Fs};
use git::Git;

pub mod agents;
pub mod mcp;

/// The binary's file name.
pub const NAME: &str = "cydonia-server";

/// What [`Command::Version`] prints.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The daemon's state directory, relative to `$HOME`, or to the directory the
/// server is run in where `$HOME` is unset.
pub const STATE_DIR: &str = ".cupola/cydonia-server";

/// The services a cydonia-server answers: Cupola's [`Fs`] and [`Exec`],
/// [`Artifact`], [`Tables`], [`Git`] and [`Agents`].
pub struct Services;

/// What the binary is run as, by its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `version`: prints [`VERSION`].
    Version,
    /// `proxy start` or `proxy reconnect`: bridges stdio to the daemon.
    Proxy(Mode),
    /// `daemon`: serves [`Services`] until accepting a connection fails, and
    /// relays [`mcp::Mcp`] for every process that joins it.
    Daemon,
    /// `mcp`, then the [`Binding`]'s arguments: an MCP server on stdio,
    /// relayed through the running daemon to its client.
    Mcp(Binding),
}

impl Services {
    pub fn register(server: &Server) {
        Fs::register(server);
        Exec::register(server);
        Artifact::register(server);
        Tables::register(server);
        Git::register(server);
        Agents::register(server);
    }

    /// Runs `daemon` with a server answering [`Services`], relaying
    /// [`crate::mcp::Mcp`] for every process that joins it, until accepting a
    /// connection fails.
    #[cfg(unix)]
    pub async fn run(daemon: &cupola::daemon::Daemon) -> io::Result<()> {
        use futures::future::{self, Either};

        let (server, runner) = Server::new();
        Self::register(&server);
        let welcomed = daemon.welcome(|local| crate::mcp::Mcp::relay(local, &server));
        let served = future::try_join(daemon.run(&server), welcomed);
        match future::select(runner, std::pin::pin!(served)).await {
            Either::Left(((), _)) => Ok(()),
            Either::Right((served, _)) => served.map(drop),
        }
    }
}

impl Command {
    /// The command `args` name, the program's own name not among them.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Option<Self> {
        let args: Vec<OsString> = args.into_iter().collect();
        let args: Vec<&str> = args.iter().map(|arg| arg.to_str()).collect::<Option<_>>()?;
        match args.as_slice() {
            ["version"] => Some(Self::Version),
            ["proxy", "start"] => Some(Self::Proxy(Mode::Start)),
            ["proxy", "reconnect"] => Some(Self::Proxy(Mode::Reconnect)),
            ["daemon"] => Some(Self::Daemon),
            ["mcp", binding @ ..] => Binding::parse(binding).map(Self::Mcp),
            _ => None,
        }
    }

    /// The arguments [`Command::parse`] reads as this command.
    pub fn args(&self) -> Vec<OsString> {
        let args: &[&str] = match self {
            Self::Version => &["version"],
            Self::Proxy(Mode::Start) => &["proxy", "start"],
            Self::Proxy(Mode::Reconnect) => &["proxy", "reconnect"],
            Self::Daemon => &["daemon"],
            Self::Mcp(binding) => {
                let mut args = vec![OsString::from("mcp")];
                args.extend(binding.args());
                return args;
            }
        };
        args.iter().map(OsString::from).collect()
    }

    #[cfg(unix)]
    pub fn run(self) -> ExitCode {
        use cupola::{
            daemon::Daemon,
            proxy::{self, Launch},
        };

        let home = std::env::var_os("HOME").map(PathBuf::from);
        let daemon = Daemon::at(home.unwrap_or_default().join(STATE_DIR));
        match self {
            Self::Version => {
                println!("{VERSION}");
                ExitCode::SUCCESS
            }
            Self::Proxy(mode) => {
                let launch = match mode {
                    Mode::Reconnect => Launch::Reconnect,
                    Mode::Start => match std::env::current_exe() {
                        Ok(program) => {
                            let mut command = std::process::Command::new(program);
                            command.args(Self::Daemon.args());
                            Launch::Start(command)
                        }
                        Err(error) => return Self::failed(&proxy::Error::Io(error), 1),
                    },
                };
                match daemon.proxy(launch, io::stdin(), io::stdout()) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(error) => Self::failed(&error, error.exit_code()),
                }
            }
            Self::Daemon => match futures::executor::block_on(Services::run(&daemon)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => Self::failed(&error, 1),
            },
            Self::Mcp(binding) => {
                let relayed = futures::executor::block_on(async {
                    let connection = daemon.join().await?;
                    crate::mcp::stdio(connection, binding, io::stdin(), io::stdout()).await
                });
                match relayed {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(error) => Self::failed(&format!("{error:#}"), 1),
                }
            }
        }
    }

    #[cfg(unix)]
    fn failed(error: &dyn std::fmt::Display, code: i32) -> ExitCode {
        eprintln!("{NAME}: {error}");
        ExitCode::from(u8::try_from(code).unwrap_or(1))
    }
}
