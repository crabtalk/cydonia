//! Opt-in end-to-end SSH test. Run with scripts/test-remote-ssh.
#![cfg(unix)]

use artifact::{
    location::Location,
    project::{Project as _, remote, store::Store},
};
use cupola::{
    Client,
    client::{
        platform::Platform,
        session::{Heartbeat, Mode, Session},
        ssh::{Delegate, Host, Remote, ServerPath},
    },
    exec::{Command, Exec, Output},
    fs::{Expect, Fs, Request, Response},
};
use cydonia_server::mcp::{Binding, Mcp};
use futures::{FutureExt as _, StreamExt as _, future::BoxFuture};
use git::Backend as _;
use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

struct Binary;

struct WrongVersion;

impl Delegate for WrongVersion {
    fn name(&self) -> &str {
        "cydonia-ssh-wrong-version"
    }
    fn version(&self) -> &str {
        "0.0.0-wrong"
    }
    fn version_args(&self) -> Vec<OsString> {
        Binary.version_args()
    }
    fn proxy_args(&self, mode: Mode) -> Vec<OsString> {
        Binary.proxy_args(mode)
    }
    fn binary(&self, platform: Platform) -> BoxFuture<'static, anyhow::Result<PathBuf>> {
        Binary.binary(platform)
    }
    fn ask(&self, _: String) -> BoxFuture<'static, Option<String>> {
        async { None }.boxed()
    }
}

#[test]
#[ignore = "requires the isolated sshd fixture: scripts/test-remote-ssh"]
fn an_uploaded_version_mismatch_preserves_the_installed_server() {
    use cupola::client::session::Transport as _;
    use std::os::unix::fs::PermissionsExt as _;
    let home = PathBuf::from(
        std::env::var_os("CYDONIA_SSH_TEST_HOME").expect("run scripts/test-remote-ssh"),
    );
    let port = std::env::var("CYDONIA_SSH_TEST_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let user = std::env::var("CYDONIA_SSH_TEST_USER").unwrap();
    let installed =
        home.join(ServerPath::new(WrongVersion.name(), WrongVersion.version()).as_path());
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    let previous = b"#!/bin/sh\necho previous\n";
    std::fs::write(&installed, previous).unwrap();
    std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755)).unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let transport = Remote::new(
            Host::new(format!("{user}@127.0.0.1")).unwrap().port(port),
            WrongVersion,
        )
        .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(20), transport.open(Mode::Start))
            .await
            .unwrap();
        let error = match result {
            Ok(_) => panic!("a wrong version was installed"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("does not match"), "{error:#}");
    });
    assert_eq!(std::fs::read(installed).unwrap(), previous);
}

impl Delegate for Binary {
    fn name(&self) -> &str {
        "cydonia-ssh-test-server"
    }
    fn version(&self) -> &str {
        cydonia_server::VERSION
    }
    fn version_args(&self) -> Vec<OsString> {
        cydonia_server::Command::Version.args()
    }
    fn proxy_args(&self, mode: Mode) -> Vec<OsString> {
        cydonia_server::Command::Proxy(mode).args()
    }
    fn binary(&self, platform: Platform) -> BoxFuture<'static, anyhow::Result<PathBuf>> {
        async move {
            anyhow::ensure!(
                Some(platform) == Platform::local(),
                "the fixture must run on this machine"
            );
            Ok(PathBuf::from(env!("CARGO_BIN_EXE_cydonia-server")))
        }
        .boxed()
    }
    fn ask(&self, _: String) -> BoxFuture<'static, Option<String>> {
        async { None }.boxed()
    }
}

async fn run(exec: &cupola::Handle<Exec>, command: Command) -> String {
    let mut process = exec.start(&command).await.unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut code = None;
    while let Some(output) = process.next().await {
        match output.unwrap() {
            Output::Stdout(bytes) => stdout.extend(bytes),
            Output::Stderr(bytes) => stderr.extend(bytes),
            Output::Exit(exit) => code = exit,
        }
    }
    assert_eq!(code, Some(0), "{}", String::from_utf8_lossy(&stderr));
    String::from_utf8(stdout).unwrap()
}

#[test]
#[ignore = "requires the isolated sshd fixture: scripts/test-remote-ssh"]
fn ssh_installs_the_server_and_serves_files_git_projects_processes_and_mcp() {
    let home = PathBuf::from(
        std::env::var_os("CYDONIA_SSH_TEST_HOME").expect("run scripts/test-remote-ssh"),
    );
    let port = std::env::var("CYDONIA_SSH_TEST_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let user = std::env::var("CYDONIA_SSH_TEST_USER").unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(45), async {
            let transport = Remote::new(Host::new(format!("{user}@127.0.0.1")).unwrap().port(port), Binary).unwrap();
            let (client, runner) = Client::new();
            let tools = mcp::Server::new().mount(&mcp::tools::board::TOOLS);
            Mcp::serve(&client, move |request| tools.message(&request.message, request.binding.project.as_ref(), request.binding.session.as_deref()).text());
            let runner = tokio::spawn(runner);
            let session = Arc::new(Session::with_heartbeat(client.clone(), transport, Heartbeat { interval: Duration::from_millis(100), timeout: Duration::from_secs(2), missed: 2 }));
            let serving = tokio::spawn({ let session = session.clone(); async move { let Err(error) = session.run().await; panic!("SSH session failed: {error:#}") } });
            let exec = client.service::<Exec>();
            let root = home.join("project with spaces");
            let mut setup = Command::new("/bin/sh");
            setup.args = vec!["-c".into(), "test \"$HOME\" = \"$1\" && mkdir -p \"$2\" && git -C \"$2\" init -q && printf ready".into(), "sh".into(), home.as_os_str().into(), root.as_os_str().into()];
            assert_eq!(run(&exec, setup).await, "ready");
            let files = client.service::<Fs>();
            let path = root.join("hello.txt");
            assert_eq!(files.request(&Request::Write { path: path.clone(), bytes: b"over ssh".to_vec(), expect: Expect::Absent }).await.unwrap(), Response::Written);
            assert_eq!(files.request(&Request::Read { path, limit: None }).await.unwrap(), Response::Read { bytes: b"over ssh".to_vec(), cut: false });
            assert!(git::Remote::new(&client).status(&root).await.unwrap().is_some());
            let (project, refresh) = remote::Project::open(client.clone(), root.clone()).await.unwrap();
            let refreshing = tokio::spawn(refresh);
            project.create_board("SSH roadmap", "SSH").await.unwrap();
            let location = Location::Remote { host: "42".parse().unwrap(), root };
            mcp::rail::install(|_| {});
            mcp::rail::set_open(vec![location.clone()]);
            let held = location.clone();
            mcp::rail::install_stores(move |at| (*at == held).then(|| Store::Remote(project.clone())));
            let mut agent = Command::new(home.join(ServerPath::new(Binary.name(), Binary.version()).as_path()).into_os_string());
            agent.args = cydonia_server::Command::Mcp(Binding { project: Some(location), session: None }).args();
            let mut process = exec.start(&agent).await.unwrap();
            process.write(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"board_list\",\"arguments\":{}}}\n").await.unwrap();
            process.close_stdin().await.unwrap();
            let mut output = Vec::new();
            while let Some(event) = process.next().await {
                if let Output::Stdout(bytes) = event.unwrap() { output.extend(bytes); }
            }
            let answer: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert!(answer["error"].is_null(), "{answer}");
            assert!(answer["result"].to_string().contains("SSH roadmap"), "{answer}");
            refreshing.abort();
            serving.abort();
            runner.abort();
        }).await.expect("SSH end-to-end test must finish");
    });
}
