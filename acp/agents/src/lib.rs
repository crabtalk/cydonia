//! An ACP agent as a program on this machine: find it in the catalog, and put
//! it on disk.
//!
//! The registry is the protocol's own catalog, pinned to exact versions — so an
//! agent's build never changes underfoot the way `npx <pkg>@latest` does.
//! Agents install once into a data directory and run straight from there,
//! keeping package managers out of the chat path.
//!
//! ```no_run
//! # fn main() -> anyhow::Result<()> {
//! # let (cache_dir, data_dir) = (std::path::Path::new("cache"), std::path::Path::new("data"));
//! let catalog = cydonia_acp_agents::registry::catalog(cache_dir).expect("a catalog");
//! let agent = &catalog.agents[0];
//!
//! let installed = match cydonia_acp_agents::Installed::find(data_dir, &agent.id) {
//!     Some(installed) => installed,
//!     None => agent.install(data_dir, |line| println!("{line}"))?,
//! };
//!
//! println!("{} {}", installed.command, installed.args.join(" "));
//! # Ok(()) }
//! ```
//!
//! Hand that command to [`cydonia_acp::spawn`] with whatever working directory and
//! stderr the caller wants. Everything here blocks — it reaches the network and
//! runs `npm` — so call it off a worker rather than inside a turn.
//!
//! [`cydonia_acp::spawn`]: https://docs.rs/cydonia-acp

#[cfg(feature = "install")]
pub use install::Installed;
pub use registry::{Agent, Distribution, Registry};
pub use utils::{contained, package_name};

pub mod mcp;
pub mod registry;

#[cfg(feature = "install")]
mod install;
mod utils;
