//! The MCP server this app answers as — the other direction from [`super::mcp`],
//! which is the servers it dials.
//!
//! One door for the whole app, on a port the system chose. There is nothing to
//! configure: the URL goes to the agents this app launches, and a tool is told
//! which directory it is about on the call rather than by which port it arrived
//! on. The settings window shows the address because knowing where a thing is
//! listening is worth something, not because anybody has to type it.

use mcp::{Server, http::Door, tools};
use std::path::Path;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

/// The door, while it is open. Dropping it closes the port, so shutting the
/// door is emptying this and nothing else.
static DOOR: OnceLock<Mutex<Option<Door>>> = OnceLock::new();

fn held() -> &'static Mutex<Option<Door>> {
    DOOR.get_or_init(Mutex::default)
}

/// Whether an agent may change a project, shared with the server that answers
/// them. A flag rather than a rebuild: the door hands out one URL, and moving
/// this must not be what takes it away.
static WRITE: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn write() -> &'static Arc<AtomicBool> {
    WRITE.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

/// Offer the tools that change a project, or withhold them. Takes effect on
/// the next call, with nothing to reopen.
pub fn set_write(on: bool) {
    write().store(on, Ordering::Relaxed);
}

static DELETE: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn delete() -> &'static Arc<AtomicBool> {
    DELETE.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

/// Offer the tools that take an entry off the disk, or withhold them. Reads
/// the same way [`set_write`] does, and is read after it: a server that may
/// not change a project may not empty one either.
pub fn set_delete(on: bool) {
    delete().store(on, Ordering::Relaxed);
}

/// Whether the browser tools that read pages, and the browser resource, are
/// offered.
static BROWSER: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn browser() -> &'static Arc<AtomicBool> {
    BROWSER.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

/// Whether the browser tools that click and type are offered.
static BROWSER_ACT: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn browser_act() -> &'static Arc<AtomicBool> {
    BROWSER_ACT.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

/// Offer the browser tools, or withhold them: `read` the ones that read
/// pages, `act` the ones that click and type, which also need `read`. Never
/// on where the build has no browser.
pub fn set_browser(read: bool, act: bool) {
    let read = read && cfg!(not(target_os = "linux"));
    browser().store(read, Ordering::Relaxed);
    browser_act().store(read && act, Ordering::Relaxed);
}

/// The resources of surfaces switched off, for the catalog a session is sent.
pub fn hidden_resources() -> Vec<&'static str> {
    match browser().load(Ordering::Relaxed) {
        true => Vec::new(),
        false => vec![BROWSER_RESOURCE],
    }
}

const BROWSER_RESOURCE: &str = "browser";

/// Open the door, or close it. Idempotent, because the switches that reach it
/// move for their own reasons and most moves are not about this.
///
/// Not from the runtime's own threads — [`super::acp`]'s workers are where
/// every agent call runs, and `block_on` inside one of those panics.
pub fn serve(open: bool) {
    let Ok(mut door) = held().lock() else {
        return;
    };
    match open {
        false => *door = None,
        true if door.is_some() => {}
        true => {
            *door = super::acp::runtime()
                .block_on(mcp::http::open(Arc::new(server())))
                .ok();
        }
    }
}

/// Where the tools are, while the door is open — for an agent about to be
/// pointed at them, and for the row that shows it.
pub fn url() -> Option<String> {
    let door = held().lock().ok()?;
    door.as_ref().map(|door| door.url().to_owned())
}

/// The header that tells the door which project a session is, ready to be put
/// on the wire.
///
/// Here rather than at the call site: inside `agent/`, `mcp` is this app's own
/// module of that name, and reaching the crate from there reads as a typo.
pub fn project(cwd: &Path) -> (&'static str, String) {
    (mcp::http::PROJECT, mcp::http::encoded(cwd))
}

/// The header that tells the door which session is calling, by the id it is
/// filed under.
pub fn session(record: &str) -> (&'static str, String) {
    (mcp::http::SESSION, record.to_owned())
}

/// What is on it. Articles and boards; a tool set per surface as they arrive,
/// and the rail the app holds them on — see [`mcp::rail`], which the workspace
/// is what answers.
fn server() -> Server {
    Server::new()
        .mount(&tools::article::TOOLS)
        .mount(&tools::board::TOOLS)
        .mount(&tools::project::TOOLS)
        .mount(&tools::session::TOOLS)
        .mount(&tools::workspace::TOOLS)
        .mount_switched(
            tools::browser::looking(),
            BROWSER_RESOURCE,
            browser().clone(),
        )
        .mount_switched(
            tools::browser::acting(),
            BROWSER_RESOURCE,
            browser_act().clone(),
        )
        .writable(write().clone())
        .deletes(delete().clone())
}
