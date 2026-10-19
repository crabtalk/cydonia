#![cfg(unix)]

mod fixture;

use cydonia_cupola::{
    Client, Error,
    exec::{Command, Exec, Output, Process},
};
use fixture::{attach, builtin_server};
use futures::StreamExt as _;

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args = vec!["-c".into(), script.into()];
    command
}

/// Everything a process wrote to each stream, and its exit code.
async fn finish(mut process: Process) -> (Vec<u8>, Vec<u8>, Option<i32>) {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    while let Some(output) = process.next().await {
        match output.unwrap() {
            Output::Stdout(bytes) => stdout.extend(bytes),
            Output::Stderr(bytes) => stderr.extend(bytes),
            Output::Exit(code) => {
                assert!(process.next().await.is_none());
                return (stdout, stderr, code);
            }
        }
    }
    panic!("events ended before the exit");
}

#[tokio::test]
async fn a_program_gets_stdin_and_reports_stdout_stderr_and_exit() {
    let dir = tempfile::tempdir().unwrap();
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let mut command =
        shell(r#"read line; echo "out:$line:$(pwd)"; echo "err:$CUPOLA_TEST" >&2; exit 3"#);
    command.cwd = Some(dir.path().canonicalize().unwrap());
    command.env = vec![("CUPOLA_TEST".into(), "set".into())];
    let process = client.service::<Exec>().start(&command).await.unwrap();
    process.write(b"hello\n").await.unwrap();

    let (stdout, stderr, code) = finish(process).await;
    let cwd = dir.path().canonicalize().unwrap();
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        format!("out:hello:{}\n", cwd.display())
    );
    assert_eq!(stderr, b"err:set\n");
    assert_eq!(code, Some(3));
}

#[tokio::test]
async fn processes_at_once_keep_their_own_output() {
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);
    let exec = client.service::<Exec>();

    let first = exec.start(&shell("cat")).await.unwrap();
    let second = exec.start(&shell("cat")).await.unwrap();
    assert_ne!(first.id(), second.id());
    second.write(b"second").await.unwrap();
    first.write(b"first").await.unwrap();
    first.close_stdin().await.unwrap();
    second.close_stdin().await.unwrap();

    let (first, second) = futures::join!(finish(first), finish(second));
    assert_eq!(first, (b"first".to_vec(), Vec::new(), Some(0)));
    assert_eq!(second, (b"second".to_vec(), Vec::new(), Some(0)));
}

#[tokio::test]
async fn a_killed_process_exits_without_a_code() {
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let process = client
        .service::<Exec>()
        .start(&shell("sleep 30"))
        .await
        .unwrap();
    process.kill().await.unwrap();
    let (_, _, code) = finish(process).await;
    assert_eq!(code, None);
}

#[tokio::test]
async fn a_program_that_cannot_start_is_an_error() {
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let error = client
        .service::<Exec>()
        .start(&Command::new("/nonexistent/cupola-test"))
        .await
        .err()
        .unwrap();
    assert!(
        matches!(&error, Error::Remote { message, .. } if message.starts_with("start /nonexistent/cupola-test: ")),
        "{error:?}"
    );
}

#[tokio::test]
async fn an_input_writes_while_its_process_streams_elsewhere() {
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let process = client.service::<Exec>().start(&shell("cat")).await.unwrap();
    let input = process.input();
    assert_eq!(input.id(), process.id());
    let finished = tokio::spawn(finish(process));
    input.write(b"through the input").await.unwrap();
    input.close_stdin().await.unwrap();
    let (stdout, _, code) = finished.await.unwrap();
    assert_eq!(stdout, b"through the input");
    assert_eq!(code, Some(0));
}
