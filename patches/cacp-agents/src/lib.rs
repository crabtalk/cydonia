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
//! let catalog = cacp_agents::registry::catalog(cache_dir).expect("a catalog");
//! let agent = &catalog.agents[0];
//!
//! let installed = match cacp_agents::Installed::find(data_dir, &agent.id) {
//!     Some(installed) => installed,
//!     None => agent.install(data_dir, |line| println!("{line}"))?,
//! };
//!
//! println!("{} {}", installed.command, installed.args.join(" "));
//! # Ok(()) }
//! ```
//!
//! Hand that command to [`cacp::spawn`] with whatever working directory and
//! stderr the caller wants. Everything here blocks — it reaches the network and
//! runs `npm` — so call it off a worker rather than inside a turn.
//!
//! [`cacp::spawn`]: https://docs.rs/cacp

pub use install::{Installed, package_name};
pub use registry::{Agent, Distribution, Registry};
pub use utils::contained;

pub mod mcp;
pub mod registry;

mod install;
mod utils;
