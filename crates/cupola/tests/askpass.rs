//! A helper's prompt reaching the askpass that serves it, as ssh's askpass
//! program would send it.

#![cfg(unix)]

use std::{io, path::Path, thread};

use cydonia_cupola::client::askpass::{Askpass, Helper, SOCKET_VAR};
use futures::{FutureExt as _, StreamExt as _, channel::mpsc};

/// Runs `helper` on a thread with `prompt`, answering with its output.
fn ask(helper: Helper, prompt: &'static [u8]) -> thread::JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        helper.answer(prompt, &mut output).map(|()| output)
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_is_answered_by_the_embedder() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("askpass.sock");
    let askpass = Askpass::bind(&socket).unwrap();
    let (asked, mut prompts) = mpsc::unbounded();

    let first = ask(Helper::at(&socket), b"me@box's password: ");
    let serving = askpass.serve(move |prompt| {
        asked.unbounded_send(prompt.clone()).unwrap();
        async move { Some(format!("secret for {prompt}")) }.boxed()
    });
    let answered = async {
        let first = tokio::task::spawn_blocking(move || first.join().unwrap())
            .await
            .unwrap();
        let second = ask(Helper::at(&socket), b"Verification code: ");
        let second = tokio::task::spawn_blocking(move || second.join().unwrap())
            .await
            .unwrap();
        (first, second)
    };
    let (first, second) = tokio::select! {
        answered = answered => answered,
        served = serving => panic!("serving ended: {served:?}"),
    };
    assert_eq!(first.unwrap(), b"secret for me@box's password: ");
    assert_eq!(second.unwrap(), b"secret for Verification code: ");
    assert_eq!(prompts.next().await.unwrap(), "me@box's password: ");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_prompt_fails_the_helper_and_ends_serving() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("askpass.sock");
    let askpass = Askpass::bind(&socket).unwrap();

    let helper = ask(Helper::at(&socket), b"Password: ");
    let served = askpass.serve(|_| async { None }.boxed()).await;
    assert!(served.is_err());
    let error = tokio::task::spawn_blocking(move || helper.join().unwrap())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[test]
fn ssh_is_told_to_run_the_program_as_the_helper() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("askpass.sock");
    let askpass = Askpass::bind(&socket).unwrap();
    let env = askpass.env(Path::new(
        "/Applications/Cydonia.app/Contents/MacOS/cydonia",
    ));
    assert_eq!(
        env,
        [
            (
                "SSH_ASKPASS".into(),
                "/Applications/Cydonia.app/Contents/MacOS/cydonia".into()
            ),
            ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
            (SOCKET_VAR.into(), socket.into_os_string()),
        ]
    );
}
