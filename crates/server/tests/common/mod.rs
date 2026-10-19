//! A server with cydonia-server's services in this process, its client over
//! Cupola's mock transport, and projects in temporary directories.

// Every test binary compiles this module whole and uses a part of it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use cupola::{Client, Server, mock::Wire};
use cydonia_server::Services;
use tokio::runtime::Runtime;

pub struct Scratch(PathBuf);

/// A client, the server it reaches, and the wire between them.
pub struct Served {
    pub runtime: Runtime,
    pub client: Client,
    pub server: Server,
    _wire: Wire,
}

impl Scratch {
    pub fn new(name: &str) -> Self {
        Self::under(&std::env::temp_dir(), name)
    }

    /// Under `/tmp`, for a unix socket a few directories down: its path must
    /// fit in about a hundred bytes.
    pub fn short(name: &str) -> Self {
        Self::under(Path::new("/tmp"), name)
    }

    fn under(base: &Path, name: &str) -> Self {
        let path = base.join(format!("cydonia-server-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Served {
    pub fn new() -> Self {
        let runtime = Runtime::new().unwrap();
        let (server, runner) = Server::new();
        Services::register(&server);
        runtime.spawn(runner);
        let (client, runner) = Client::new();
        runtime.spawn(runner);
        let (wire, near, far) = Wire::pair();
        let attached = client.clone();
        runtime.spawn(async move {
            let _ = attached.attach(near).await;
        });
        let attaching = server.clone();
        runtime.spawn(async move {
            let _ = attaching.attach(far).await;
        });
        Self {
            runtime,
            client,
            server,
            _wire: wire,
        }
    }
}
