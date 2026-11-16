#![cfg(all(feature = "desktop", unix))]

use cydonia_gui::model::terminal::process::Process;
use futures::StreamExt;
use portable_pty::{CommandBuilder, PtySize};
use std::{sync::mpsc, time::Duration};

#[test]
fn native_command_preserves_arguments_environment_directory_resize_and_exit() {
    let cwd = std::env::temp_dir().canonicalize().unwrap();
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "read ready; printf 'ARG:%s|ENV:%s|CWD:%s\\n' \"$1\" \"$CYDONIA_TEST_VALUE\" \"$PWD\"; stty size; exit 7", "agent", "value with spaces"]);
    command.env("CYDONIA_TEST_VALUE", "native environment");
    let (mut process, mut output) = Process::spawn(&cwd, command).unwrap();
    process
        .master
        .resize(PtySize {
            rows: 37,
            cols: 101,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    process.input.send(b"ready\n".to_vec()).unwrap();
    let exit = process.exit.take().unwrap();
    let (done, result) = mpsc::channel();
    std::thread::spawn(move || {
        let result = futures::executor::block_on(async move {
            let mut bytes = Vec::new();
            while let Some(chunk) = output.next().await {
                bytes.extend(chunk);
            }
            (bytes, exit.await.unwrap().unwrap())
        });
        let _ = done.send(result);
    });
    let (bytes, exit) = result.recv_timeout(Duration::from_secs(10)).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("ARG:value with spaces|ENV:native environment"),
        "{text}"
    );
    assert!(text.contains(&format!("CWD:{}", cwd.display())), "{text}");
    assert!(text.contains("37 101"), "{text}");
    assert_eq!(exit.exit_code(), 7);
    assert!(!exit.success());
}

#[test]
fn missing_interactive_executable_reports_launch_failure() {
    let command = CommandBuilder::new("/no/such/cydonia-agent");
    assert!(Process::spawn(&std::env::temp_dir(), command).is_err());
}

#[test]
fn dropping_the_process_terminates_a_running_agent() {
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "read forever"]);
    let (mut process, _output) = Process::spawn(&std::env::temp_dir(), command).unwrap();
    let exit = process.exit.take().unwrap();
    drop(process);
    let (done, result) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(futures::executor::block_on(exit));
    });
    assert!(
        result
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap()
            .is_ok()
    );
}
