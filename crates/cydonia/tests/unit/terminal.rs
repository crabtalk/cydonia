use super::*;
use std::time::Duration;

fn shell() -> (Shell, mpsc::Receiver<Vec<u8>>) {
    let mut command = CommandBuilder::new("/bin/sh");
    command.arg("-i");
    command.env("ENV", "/dev/null");
    Shell::open_command(Path::new("/private/tmp"), command).expect("open PTY")
}

fn collect(mut output: mpsc::Receiver<Vec<u8>>) -> channel::Receiver<Vec<u8>> {
    let (tx, rx) = channel::channel();
    std::thread::spawn(move || {
        let bytes = futures::executor::block_on(async {
            let mut bytes = Vec::new();
            while let Some(chunk) = output.next().await {
                bytes.extend(chunk);
            }
            bytes
        });
        let _ = tx.send(bytes);
    });
    rx
}

#[test]
fn shell_has_session_directory_terminal_environment_and_resizes() {
    let (shell, output) = shell();
    let output = collect(output);
    shell
        .master
        .resize(PtySize {
            rows: 37,
            cols: 101,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    shell
        .input
        .send(b"printf '\\nCWD:%s\\nTERM:%s\\n' \"$PWD\" \"$TERM\"; stty size; exit\n".to_vec())
        .unwrap();
    let bytes = output
        .recv_timeout(Duration::from_secs(10))
        .expect("shell exits and closes output");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("CWD:/private/tmp\r\n"), "{text}");
    assert!(text.contains("TERM:xterm-256color\r\n"), "{text}");
    assert!(text.contains("37 101\r\n"), "{text}");
}

#[test]
fn dropping_shell_closes_its_output() {
    let (shell, output) = shell();
    let output = collect(output);
    drop(shell);
    output
        .recv_timeout(Duration::from_secs(10))
        .expect("dropping a session ends its shell");
}

#[gpui::test]
fn tabs_keep_shells_and_close_on_exit(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Dark, cx));
    let window =
        cx.add_window(|window, cx| TerminalPanel::new(Path::new("/private/tmp"), window, cx));
    let panel = window.root(cx).unwrap();
    let empty = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed = empty.clone();
    let _subscription =
        cx.update(|cx| cx.subscribe(&panel, move |_, _: &Empty, _| observed.set(true)));
    window
        .update(cx, |panel, window, cx| {
            let first = panel.tabs[0].terminal.clone();
            panel.add(window, cx);
            panel.add(window, cx);
            assert_eq!(panel.tabs.len(), 3);
            assert_eq!(panel.active, 3);
            assert_eq!(panel.tabs[0].terminal, first);
            panel.close(2, window, cx);
            assert_eq!(panel.active, 3);
            panel.close(3, window, cx);
            assert_eq!(panel.active, 1);
            assert!(panel.focus_handle(cx).is_focused(window));
            first.update(cx, |_, cx| cx.emit(Exited));
        })
        .unwrap();
    cx.run_until_parked();
    assert!(empty.get());
    panel.read_with(cx, |panel, _| assert!(panel.tabs.is_empty()));
}

#[gpui::test]
fn background_exit_preserves_active_tab_and_focus(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Dark, cx));
    let window =
        cx.add_window(|window, cx| TerminalPanel::new(Path::new("/private/tmp"), window, cx));
    window
        .update(cx, |panel, window, cx| {
            panel.add(window, cx);
            let background = panel.tabs[0].terminal.clone();
            background.update(cx, |_, cx| cx.emit(Exited));
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |panel, window, cx| {
            assert_eq!(panel.tabs.len(), 1);
            assert_eq!(panel.active, 2);
            assert!(panel.focus_handle(cx).is_focused(window));
            let elsewhere = cx.focus_handle();
            window.focus(&elsewhere, cx);
            panel.add(window, cx);
            window.focus(&elsewhere, cx);
            panel.close(3, window, cx);
            assert!(elsewhere.is_focused(window));
        })
        .unwrap();
}

#[test]
fn directory_labels_use_the_current_folder_name() {
    for (directory, expected) in [
        ("/work/project", "project"),
        ("/work/project/src", "src"),
        ("/work/project/src/ui", "ui"),
        ("/work/project-other", "project-other"),
        ("/tmp", "tmp"),
        ("/", "/"),
    ] {
        assert_eq!(directory_label(Path::new(directory)), expected);
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn directory_tracking_reads_the_shell_after_cd() {
    use std::{
        io::BufRead,
        process::{Command, Stdio},
    };
    let mut child = Command::new("/bin/sh")
        .args(["-c", "cd / && printf 'ready\\n'; read value"])
        .current_dir("/tmp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    let directory = process_directory(child.id());
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(ready, "ready\n");
    assert_eq!(directory.as_deref(), Some(Path::new("/")));
}
