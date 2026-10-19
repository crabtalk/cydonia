//! The `agents` service, over [`common::Served`], installing a fake agent from
//! a catalog in the server's cache, its release served on loopback. The
//! server's directories come from `XDG_*_HOME`, which are this process's, so
//! this file holds one test.

#![cfg(unix)]

mod common;

use std::{
    io::{BufRead as _, BufReader, Write as _},
    net::TcpListener,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use common::{Scratch, Served};
use cupola::Error;
use cydonia_server::agents::Remote;

/// The fake agent's release: a bare executable.
const RELEASE: &str = "#!/bin/sh\necho \"fake $1 $FAKE\"\n";

/// Every platform key a test machine has, each naming the same release.
const PLATFORMS: [&str; 4] = [
    "darwin-aarch64",
    "darwin-x86_64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Serves [`RELEASE`] to every request on loopback, counting them. Answers
/// the release's URL.
fn serve(hits: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/fake-agent", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                line.clear();
            }
            hits.fetch_add(1, Ordering::SeqCst);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{RELEASE}",
                RELEASE.len()
            );
        }
    });
    url
}

/// A catalog with one agent, `fake`, in `cache`.
fn catalog(cache: &Path, archive: &str) {
    let build = format!(
        r#"{{"archive":"{archive}","cmd":"fake-agent","args":["--acp"],"env":{{"FAKE":"yes"}}}}"#
    );
    let builds: Vec<String> = PLATFORMS
        .iter()
        .map(|platform| format!(r#""{platform}":{build}"#))
        .collect();
    let body = format!(
        r#"{{"version":"1.0.0","agents":[{{"id":"fake","name":"Fake","version":"1.2.3","distribution":{{"binary":{{{}}}}}}}]}}"#,
        builds.join(",")
    );
    std::fs::create_dir_all(cache).unwrap();
    std::fs::write(cache.join("registry.json"), body).unwrap();
}

#[test]
fn agents_install_on_the_server_by_catalog_id() {
    let scratch = Scratch::new("agents");
    let (config, data) = (scratch.path().join("config"), scratch.path().join("data"));
    // SAFETY: the one test in this binary, before it starts a thread.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    let hits = Arc::new(AtomicUsize::new(0));
    let archive = serve(hits.clone());
    catalog(&config.join("cydonia").join("cache"), &archive);

    let served = Served::new();
    let agents = Remote::new(&served.client);
    served.runtime.block_on(async {
        let mut lines = Vec::new();
        let installed = agents
            .resolve("fake", |line| lines.push(line.to_owned()))
            .await
            .unwrap();
        assert_eq!(installed.id, "fake");
        assert_eq!(installed.version, "1.2.3");
        assert_eq!(installed.args, ["--acp"]);
        assert_eq!(installed.env.get("FAKE").map(String::as_str), Some("yes"));
        let command = data.join("cydonia/agents/fake/fake-agent");
        assert_eq!(Path::new(&installed.command), command);
        assert_eq!(lines.first(), Some(&format!("downloading {archive}")));
        assert_eq!(lines.last().map(String::as_str), Some("installed"));
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let ran = std::process::Command::new(&installed.command)
            .args(&installed.args)
            .envs(&installed.env)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&ran.stdout), "fake --acp yes\n");

        let mut again = Vec::new();
        let found = agents
            .resolve("fake", |line| again.push(line.to_owned()))
            .await
            .unwrap();
        assert_eq!(found.command, installed.command);
        assert!(again.is_empty(), "{again:?}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let missing = agents.resolve("missing", |_| {}).await;
        match missing {
            Err(Error::Remote { message, .. }) => {
                assert!(message.contains("not in the agent catalog"), "{message}")
            }
            other => panic!("{other:?}"),
        }
    });
}
