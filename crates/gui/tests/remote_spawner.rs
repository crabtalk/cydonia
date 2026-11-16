//! `spawner::Remote` against cydonia-server's services in this process, over
//! Cupola's mock transport. The server's `$SHELL` is a script that sets an
//! environment of its own, standing in for the remote login shell. `$SHELL`
//! is this process's, so this file holds one test.

#![cfg(unix)]

use std::{collections::BTreeMap, os::unix::fs::PermissionsExt as _, path::Path, time::Duration};

use cupola::{Client, Server, exec::Exec, mock::Wire};
use cydonia_gui::{
    agent::{
        acp::{self, Launch, Session},
        spawner::{Remote, Spawner as _, Spec},
    },
    model::settings,
};
use server::Services;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

const PATIENCE: Duration = Duration::from_secs(10);

/// The login shell: sets what only it sets, prints a banner, then runs the
/// command its last argument names.
const SHELL: &str = r#"#!/bin/sh
PATH="$(dirname "$0")/bin:/usr/bin:/bin"
export PATH
CYDONIA_LOGIN=yes
export CYDONIA_LOGIN
CYDONIA_LINES="one
two"
export CYDONIA_LINES
echo "welcome back"
for word; do command="$word"; done
exec /bin/sh -c "$command"
"#;

/// On the login `PATH` alone. Echoes a line of stdin, its first argument,
/// two variables and its working directory, and writes to stderr.
const ECHO: &str = r#"#!/bin/sh
read line
printf '%s|%s|%s|%s|%s\n' "$line" "$1" "$CYDONIA_LOGIN" "$GREETING" "$(pwd)"
echo "to stderr" >&2
"#;

/// An ACP agent answering `initialize` and `session/new`.
const AGENT: &str = r#"
import json, sys
for line in sys.stdin:
    req = json.loads(line)
    if 'id' not in req:
        continue
    method = req.get('method')
    if method == 'initialize':
        fs = req['params'].get('clientCapabilities', {}).get('fs', {})
        assert not fs.get('readTextFile', False), fs
        assert not fs.get('writeTextFile', False), fs
        result = {'protocolVersion': 1, 'agentCapabilities': {}, 'authMethods': []}
    elif method == 'session/new':
        result = {'sessionId': 'remote'}
    else:
        print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'error': {'code': -32601, 'message': 'unsupported'}}), flush=True)
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;

fn script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A client attached to a server of its own over a mock wire, on
/// [`acp::runtime`].
fn exec() -> (cupola::Handle<Exec>, Wire) {
    let runtime = acp::runtime();
    let (server, runner) = Server::new();
    Services::register(&server);
    runtime.spawn(runner);
    let (client, _) = Client::new();
    let (wire, near, far) = Wire::pair();
    let attached = client.clone();
    runtime.spawn(async move {
        let _ = attached.attach(near).await;
    });
    runtime.spawn(async move {
        let _ = server.attach(far).await;
    });
    (client.service::<Exec>(), wire)
}

#[test]
fn a_remote_runs_agents_in_its_login_environment() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let bin = home.join("bin");
    std::fs::create_dir(&bin).unwrap();
    script(&home.join("shell"), SHELL);
    script(&bin.join("cydonia-echo"), ECHO);
    // SAFETY: the one test in this binary, before it starts a thread.
    unsafe { std::env::set_var("SHELL", home.join("shell")) };

    let (exec, _wire) = exec();
    acp::runtime().block_on(async {
        let remote = Remote::connect(exec).await.unwrap();
        var_answers_from_the_login_shell(&remote, &bin);
        a_bare_name_runs_with_args_env_cwd_and_piped_stdio(&remote, home).await;
        acp_connects_over_a_remote_process(&remote).await;
        a_remote_startup_failure_reports_stderr(&remote).await;
        dropping_a_remote_process_ends_its_output(&remote).await;
    });
}

fn var_answers_from_the_login_shell(remote: &Remote, bin: &Path) {
    assert_eq!(remote.var("CYDONIA_LOGIN").as_deref(), Some("yes"));
    assert_eq!(remote.var("CYDONIA_LINES").as_deref(), Some("one\ntwo"));
    let path = remote.var("PATH").unwrap();
    assert!(path.starts_with(bin.to_str().unwrap()), "{path}");
    assert_eq!(remote.var("CYDONIA_UNSET"), None);
}

async fn a_bare_name_runs_with_args_env_cwd_and_piped_stdio(remote: &Remote, cwd: &Path) {
    let cwd = cwd.canonicalize().unwrap();
    let env = BTreeMap::from([("GREETING".to_owned(), "hello there".to_owned())]);
    let args = ["it's one".to_owned()];
    let spec = Spec {
        program: "cydonia-echo",
        args: &args,
        env: &env,
        cwd: &cwd,
    };
    let mut process = remote.spawn(&spec).await.unwrap();
    process.stdin.write_all(b"a line\n").await.unwrap();
    process.stdin.flush().await.unwrap();

    let (mut stdout, mut stderr) = (String::new(), String::new());
    let read = async {
        process.stdout.read_to_string(&mut stdout).await.unwrap();
        process.stderr.read_to_string(&mut stderr).await.unwrap();
    };
    tokio::time::timeout(PATIENCE, read).await.unwrap();
    assert_eq!(
        stdout,
        format!("a line|it's one|yes|hello there|{}\n", cwd.display())
    );
    assert_eq!(stderr, "to stderr\n");
    tokio::time::timeout(PATIENCE, process.handle.ended())
        .await
        .unwrap();
}

async fn acp_connects_over_a_remote_process(remote: &Remote) {
    let agent = settings::Agent {
        name: "fake".into(),
        id: None,
        command: "python3".into(),
        args: vec!["-u".into(), "-c".into(), AGENT.into()],
        env: Default::default(),
    };
    let (tx, _events) = acp::channel();
    let launch = Launch::new(artifact::location::Location::Remote {
        host: "1".parse().unwrap(),
        root: std::env::temp_dir(),
    });
    let opened = Session::spawn_on(remote, &agent, launch, tx);
    let session = tokio::time::timeout(PATIENCE, opened)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.session_id.to_string(), "remote");
}

async fn a_remote_startup_failure_reports_stderr(remote: &Remote) {
    let agent = settings::Agent {
        name: "broken".into(),
        id: None,
        command: "/bin/sh".into(),
        args: vec![
            "-c".into(),
            "echo 'remote launch failed' >&2; exit 127".into(),
        ],
        env: Default::default(),
    };
    let (tx, mut events) = acp::channel();
    let launch = Launch::new(artifact::location::Location::Remote {
        host: "1".parse().unwrap(),
        root: std::env::temp_dir(),
    });
    let opened = tokio::time::timeout(PATIENCE, Session::spawn_on(remote, &agent, launch, tx))
        .await
        .expect("failed initialization must not hang");
    assert!(opened.is_err());
    tokio::time::timeout(PATIENCE, async {
        loop {
            if let Some(acp::Event::Stderr(line)) = events.recv().await {
                assert_eq!(line, "remote launch failed");
                break;
            }
        }
    })
    .await
    .expect("remote stderr survives failed initialization");
}

async fn dropping_a_remote_process_ends_its_output(remote: &Remote) {
    use tokio::io::AsyncBufReadExt as _;
    let args = vec![
        "-u".into(),
        "-c".into(),
        "import time; print('ready', flush=True); time.sleep(60)".into(),
    ];
    let env = BTreeMap::new();
    let cwd = std::env::temp_dir();
    let process = remote
        .spawn(&Spec {
            program: "python3",
            args: &args,
            env: &env,
            cwd: &cwd,
        })
        .await
        .unwrap();
    let mut stdout = tokio::io::BufReader::new(process.stdout);
    let mut ready = String::new();
    tokio::time::timeout(PATIENCE, stdout.read_line(&mut ready))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ready, "ready\n");
    drop(process.handle);
    let mut rest = Vec::new();
    tokio::time::timeout(PATIENCE, stdout.read_to_end(&mut rest))
        .await
        .expect("dropping the handle kills the remote agent")
        .unwrap();
    assert!(rest.is_empty());
}
