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

#[gpui::test]
fn command_w_closes_bottom_tabs_and_emits_empty_for_the_last(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Dark, cx);
        crate::view::keymap::bind_all(&crate::model::settings::Shortcuts::default(), cx);
    });
    let window =
        cx.add_window(|window, cx| TerminalPanel::new(Path::new("/private/tmp"), window, cx));
    window
        .update(cx, |panel, window, cx| panel.add(window, cx))
        .unwrap();
    let panel = window.root(cx).unwrap();
    let empty = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed = empty.clone();
    let _watch = cx.update(|cx| cx.subscribe(&panel, move |_, _: &Empty, _| observed.set(true)));
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(600.), px(240.)));
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-w");
    window
        .update(&mut visual, |panel, window, cx| {
            assert_eq!(panel.tabs.len(), 1);
            assert_eq!(panel.active, 1);
            assert!(panel.focus_handle(cx).is_focused(window));
        })
        .unwrap();
    assert!(!empty.get());
    visual.simulate_keystrokes("cmd-w");
    assert!(empty.get());
    window
        .update(&mut visual, |panel, _, _| assert!(panel.tabs.is_empty()))
        .unwrap();
}

#[gpui::test]
fn copy_terminal_selection_takes_priority_over_transcript(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Light, cx);
        crate::view::keymap::bind_all(&crate::model::settings::Shortcuts::default(), cx);
    });
    let view = cx.new(|cx| Terminal {
        directory: Path::new("/private/tmp").into(),
        emulator: Emulator::new(80, 24),
        shell: None,
        focus: cx.focus_handle(),
        geometry: None,
        selecting: false,
        scroll_remainder: 0.,
        status: None,
        _pump: None,
    });
    let window = cx.add_window(|window, cx| {
        window.focus(&view.focus_handle(cx), cx);
        crate::view::clipboard_tests::CopyRoot(view.clone().into())
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    view.update(&mut visual, |view, cx| {
        view.emulator.feed(b"terminal text");
        let grid = view.geometry.unwrap();
        view.select(
            grid.origin + gpui::point(px(0.), px(grid.line_h / 2.)),
            true,
        );
        view.select(
            grid.origin + gpui::point(px(grid.cell_w * 13.), px(grid.line_h / 2.)),
            false,
        );
        assert_eq!(
            view.emulator.selection_text().as_deref(),
            Some("terminal text")
        );
        cx.notify();
    });
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-c");
    visual.run_until_parked();
    visual.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("terminal text")
        );
    });
}

#[gpui::test]
fn cmd_t_adds_a_focused_tab_in_the_bottom_panel(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Dark, cx);
        crate::view::keymap::bind_all(&crate::model::settings::Shortcuts::default(), cx);
    });
    let window = cx.add_window(|window, cx| TerminalPanel::new(&std::env::temp_dir(), window, cx));
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    for count in [2, 3] {
        visual.simulate_keystrokes("cmd-t");
        visual.run_until_parked();
        window
            .update(&mut visual, |panel, window, cx| {
                assert_eq!(panel.tabs.len(), count);
                assert!(panel.focus_handle(cx).is_focused(window));
            })
            .unwrap();
    }
}
