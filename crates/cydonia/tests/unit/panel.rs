use super::*;
use bezel::theme::Appearance;

#[gpui::test]
fn review_is_reused_and_last_close_returns_to_launcher(cx: &mut gpui::TestAppContext) {
    let panel = cx.new(|cx| Panel::new(std::env::temp_dir(), cx));
    panel.update(cx, |panel, cx| {
        assert!(panel.strip.is_empty());
        panel.review(cx);
        let first = panel.strip.active().copied();
        panel.review(cx);
        assert_eq!(panel.strip.len(), 1);
        assert_eq!(panel.strip.active().copied(), first);
        panel.remove(first.unwrap(), cx);
        assert!(panel.strip.is_empty());
        assert_eq!(panel.strip.active().copied(), None);
    });
}

#[gpui::test]
fn file_tabs_deduplicate_paths_and_keep_buffers(cx: &mut gpui::TestAppContext) {
    let panel = cx.new(|cx| Panel::new(std::env::temp_dir(), cx));
    panel.update(cx, |panel, cx| {
        let path = std::env::temp_dir().join("cydonia-tab-example.txt");
        panel.open_file(path.clone(), cx);
        let first = panel.strip.active().copied();
        let Content::File(file) = &panel.ordered().next().unwrap().1.content else {
            panic!()
        };
        let original = file.clone();
        panel.review(cx);
        panel.open_file(path, cx);
        assert_eq!(panel.strip.len(), 2);
        assert_eq!(panel.strip.active().copied(), first);
        assert!(
            matches!(&panel.ordered().next().unwrap().1.content, Content::File(file) if *file == original)
        );
    });
}

#[gpui::test]
fn right_terminal_exit_closes_only_its_tab(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    window
        .update(cx, |panel, window, cx| {
            panel.terminal(window, cx);
            panel.terminal(window, cx);
            let Content::Terminal(first) = &panel.ordered().next().unwrap().1.content else {
                panic!()
            };
            first.clone().update(cx, |_, cx| cx.emit(Exited));
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |panel, window, cx| {
            assert_eq!(panel.strip.len(), 1);
            assert_eq!(panel.strip.active().copied(), Some(1));
            panel.close(1, window, cx);
            assert!(panel.strip.is_empty());
            assert_eq!(panel.strip.active().copied(), None);
        })
        .unwrap();
}

#[gpui::test]
fn launcher_and_popover_render(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(440.), px(600.)));
    visual.run_until_parked();
    window
        .update(&mut visual, |panel, _, cx| {
            panel.menu = true;
            cx.notify();
        })
        .unwrap();
    visual.run_until_parked();
}

#[gpui::test]
fn dirty_tab_close_requires_a_decision_and_keeps_failed_saves(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let path = std::env::temp_dir().join(format!("cydonia-panel-save-{}.txt", std::process::id()));
    std::fs::write(&path, "original").unwrap();
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    window
        .update(cx, |panel, window, cx| {
            panel.open_file(path.clone(), cx);
            let id = panel.strip.active().copied().unwrap();
            let Content::File(file) = &panel.ordered().next().unwrap().1.content else {
                panic!()
            };
            let file = file.clone();
            file.update(cx, |file, cx| {
                file.receive(Ok("original".into()), cx);
                file.field
                    .update(cx, |field, cx| field.set_content("local", cx));
            });
            panel.close(id, window, cx);
            assert_eq!(panel.closing, Some(id));
            assert_eq!(panel.strip.len(), 1);
            std::fs::write(&path, "agent edit").unwrap();
            assert!(!file.update(cx, |file, cx| file.save(false, cx)));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "agent edit");
            assert_eq!(panel.strip.len(), 1);
            panel.closing = None;
            assert!(file.read(cx).dirty(cx));
            panel.close(id, window, cx);
            panel.remove(id, cx);
            assert!(panel.strip.is_empty());
        })
        .unwrap();
    let _ = std::fs::remove_file(path);
}

#[gpui::test]
fn file_save_shortcut_works_in_the_panel(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        crate::view::keymap::bind_all(&crate::model::settings::Shortcuts::default(), cx);
    });
    let path = std::env::temp_dir().join(format!("cydonia-panel-key-{}.txt", std::process::id()));
    std::fs::write(&path, "original").unwrap();
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    window
        .update(cx, |panel, _, cx| {
            panel.open_file(path.clone(), cx);
            let Content::File(file) = &panel.ordered().next().unwrap().1.content else {
                panic!()
            };
            file.update(cx, |file, cx| {
                file.receive(Ok("original".into()), cx);
                file.field
                    .update(cx, |field, cx| field.set_content("edited", cx));
            });
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(440.), px(600.)));
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-s");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited");
    visual.simulate_keystrokes("cmd-w");
    window
        .update(&mut visual, |panel, _, _| assert!(panel.strip.is_empty()))
        .unwrap();
    let _ = std::fs::remove_file(path);
}

#[gpui::test]
fn files_opens_project_tree_instead_of_native_picker(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Light, cx));
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    window
        .update(cx, |panel, window, cx| {
            panel.choose(2, window, cx);
            assert!(panel.files_open);
            assert!(panel.files.is_some());
            assert!(panel.strip.is_empty());
            let tree = panel.files.clone().unwrap();
            let path = std::env::temp_dir().join("cydonia-tree-selection.md");
            tree.update(cx, |_, cx| cx.emit(super::super::files::Open(path.clone())));
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |panel, _, cx| {
            assert_eq!(panel.strip.len(), 1);
            assert!(panel.files_open);
            let Content::File(file) = &panel.ordered().next().unwrap().1.content else {
                panic!()
            };
            assert_eq!(file.read(cx).root, panel.project_root);
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(700.), px(600.)));
    visual.run_until_parked();
    visual.update(|window, cx| window.dispatch_action(Box::new(ToggleFiles), cx));
    window
        .update(&mut visual, |panel, _, _| assert!(!panel.files_open))
        .unwrap();
}

#[gpui::test]
fn command_w_closes_right_terminal_tabs_and_returns_to_launcher(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        crate::view::keymap::bind_all(&crate::model::settings::Shortcuts::default(), cx);
    });
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    window
        .update(cx, |panel, window, cx| {
            panel.terminal(window, cx);
            panel.terminal(window, cx);
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(600.), px(400.)));
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-w");
    window
        .update(&mut visual, |panel, _, _| {
            assert_eq!(panel.strip.len(), 1);
            assert_eq!(panel.strip.active().copied(), Some(0));
        })
        .unwrap();
    visual.simulate_keystrokes("cmd-w");
    window
        .update(&mut visual, |panel, window, _| {
            assert!(panel.strip.is_empty());
            assert!(panel.focus.is_focused(window));
        })
        .unwrap();
}

#[gpui::test]
fn toggling_files_collapses_and_reopens_the_same_browser(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    window
        .update(cx, |panel, window, cx| {
            panel.toggle_files(window, cx);
            assert!(panel.files_open);
            let files = panel.files.clone().unwrap();
            panel.toggle_files(window, cx);
            assert!(!panel.files_open);
            assert!(panel.focus.is_focused(window));
            panel.toggle_files(window, cx);
            assert!(panel.files_open);
            assert_eq!(panel.files.as_ref(), Some(&files));
            assert!(files.focus_handle(cx).is_focused(window));
        })
        .unwrap();
}

#[gpui::test]
fn language_status_reopens_install_prompt_in_file_panel(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let path =
        std::env::temp_dir().join(format!("cydonia-grammar-panel-{}.toml", std::process::id()));
    std::fs::write(&path, "[package]\nname = \"example\"\n".repeat(100)).unwrap();
    let window = cx.add_window(|_, cx| {
        let mut panel = Panel::new(std::env::temp_dir(), cx);
        panel.open_file(path.clone(), cx);
        panel
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(440.), px(600.)));
    visual.run_until_parked();
    for width in [180., 280., 440.] {
        visual.simulate_resize(gpui::size(px(width), px(600.)));
        visual.run_until_parked();
        let message = visual.debug_bounds("grammar-message").unwrap();
        let install = visual.debug_bounds("install-grammar").unwrap();
        let dismiss = visual.debug_bounds("dismiss-grammar").unwrap();
        let actions = visual.debug_bounds("grammar-actions").unwrap();
        assert!(
            message.right() <= actions.left() || message.bottom() <= actions.top(),
            "message overlaps actions at {width}px"
        );
        assert_eq!(install.top(), dismiss.top(), "buttons stay together");
        assert!(dismiss.right() <= install.left(), "Install is rightmost");
        // The notice pads itself by 8px.
        assert_eq!(actions.right(), px(width - 8.), "actions align right");
        for button in [install, dismiss] {
            assert!(button.left() >= px(0.) && button.right() <= px(width));
        }
        if width == 440. {
            assert!(message.right() <= actions.left(), "wide panel uses one row");
        } else {
            assert!(
                message.bottom() <= actions.top(),
                "narrow panel puts actions below"
            );
        }
    }
    let notice = visual.debug_bounds("grammar-notice").unwrap();
    assert_eq!(
        notice.bottom(),
        px(570.),
        "notice sits immediately above the status line"
    );
    let install = visual
        .debug_bounds("install-grammar")
        .expect("install prompt appears automatically");
    assert!(install.top() >= px(0.) && install.bottom() < px(570.));
    let dismiss_bounds = visual.debug_bounds("dismiss-grammar").unwrap();
    assert!(install.left() >= px(0.) && install.right() <= px(440.));
    assert!(dismiss_bounds.left() >= px(0.) && dismiss_bounds.right() <= px(440.));
    let dismiss = dismiss_bounds.center();
    visual.simulate_click(dismiss, gpui::Modifiers::default());
    visual.run_until_parked();
    assert!(visual.debug_bounds("install-grammar").is_none());
    let language = visual.debug_bounds("file-language").unwrap().center();
    visual.simulate_click(language, gpui::Modifiers::default());
    visual.run_until_parked();
    assert!(visual.debug_bounds("install-grammar").is_some());
    std::fs::remove_file(path).unwrap();
}

#[gpui::test]
fn terminal_menu_and_new_tab_use_cmd_t(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        crate::view::keymap::bind_all(&crate::model::settings::Shortcuts::default(), cx);
    });
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    window
        .update(cx, |panel, window, cx| {
            let items = Panel::items(window);
            let Item::Action { keystroke, .. } = &items[1] else {
                panic!("terminal action")
            };
            assert_eq!(keystroke.as_deref(), Some("⌘T"));
            panel.terminal(window, cx);
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-t");
    visual.run_until_parked();
    window
        .update(&mut visual, |panel, window, cx| {
            assert_eq!(panel.strip.len(), 2);
            let tab = panel.front().unwrap();
            let Content::Terminal(terminal) = &tab.content else {
                panic!("terminal tab")
            };
            assert!(terminal.focus_handle(cx).is_focused(window));
        })
        .unwrap();
}

#[gpui::test]
fn closing_the_front_tab_hands_the_front_to_its_right(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window = cx.add_window(|_, cx| Panel::new(std::env::temp_dir(), cx));
    window
        .update(cx, |panel, window, cx| {
            for _ in 0..3 {
                panel.terminal(window, cx);
            }
            panel.strip.activate(&1);
            panel.remove(1, cx);
            assert_eq!(
                panel.strip.active().copied(),
                Some(2),
                "the front goes to the closed tab's right-hand neighbour"
            );
            panel.remove(0, cx);
            assert_eq!(
                panel.strip.active().copied(),
                Some(2),
                "closing a background tab leaves the front where it is"
            );
            panel.remove(2, cx);
            assert_eq!(panel.strip.active().copied(), None);
        })
        .unwrap();
}
