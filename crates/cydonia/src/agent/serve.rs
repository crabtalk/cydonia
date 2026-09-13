//! The MCP server this app answers as — the other direction from [`super::mcp`],
//! which is the servers it dials.
//!
//! One door for the whole app, on a port the system chose. There is nothing to
//! configure: the URL goes to the agents this app launches, and a tool is told
//! which directory it is about on the call rather than by which port it arrived
//! on. The settings window shows the address because knowing where a thing is
//! listening is worth something, not because anybody has to type it.

use mcp::{Server, http::Door, tools};
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

/// What is on it. Boards today; a tool set per surface as they arrive.
fn server() -> Server {
    Server::new()
        .mount(&tools::board::TOOLS)
        .writable(write().clone())
}
