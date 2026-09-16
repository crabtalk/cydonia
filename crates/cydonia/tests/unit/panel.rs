use super::*;
use bezel::theme::Appearance;

#[gpui::test]
fn review_is_reused_and_last_close_returns_to_launcher(cx: &mut gpui::TestAppContext) {
    let panel = cx.new(|cx| Panel::new(std::env::temp_dir(), cx));
    panel.update(cx, |panel, cx| {
        assert!(panel.tabs.is_empty());
        panel.review(cx);
        let first = panel.active;
        panel.review(cx);
        assert_eq!(panel.tabs.len(), 1);
        assert_eq!(panel.active, first);
        panel.remove(first.unwrap(), cx);
        assert!(panel.tabs.is_empty());
        assert_eq!(panel.active, None);
    });
}

#[gpui::test]
fn file_tabs_deduplicate_paths_and_keep_buffers(cx: &mut gpui::TestAppContext) {
    let panel = cx.new(|cx| Panel::new(std::env::temp_dir(), cx));
    panel.update(cx, |panel, cx| {
        let path = std::env::temp_dir().join("cydonia-tab-example.txt");
        panel.open_file(path.clone(), cx);
        let first = panel.active;
        let Content::File(file) = &panel.tabs[0].content else {
            panic!()
        };
        let original = file.clone();
        panel.review(cx);
        panel.open_file(path, cx);
        assert_eq!(panel.tabs.len(), 2);
        assert_eq!(panel.active, first);
        assert!(matches!(&panel.tabs[0].content, Content::File(file) if *file == original));
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
            let Content::Terminal(first) = &panel.tabs[0].content else {
                panic!()
            };
            first.clone().update(cx, |_, cx| cx.emit(Exited));
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |panel, window, cx| {
            assert_eq!(panel.tabs.len(), 1);
            assert_eq!(panel.active, Some(1));
            panel.close(1, window, cx);
            assert!(panel.tabs.is_empty());
            assert_eq!(panel.active, None);
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
            let id = panel.active.unwrap();
            let Content::File(file) = &panel.tabs[0].content else {
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
            assert_eq!(panel.tabs.len(), 1);
            std::fs::write(&path, "agent edit").unwrap();
            assert!(!file.update(cx, |file, cx| file.save(false, cx)));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "agent edit");
            assert_eq!(panel.tabs.len(), 1);
            panel.closing = None;
            assert!(file.read(cx).dirty(cx));
            panel.close(id, window, cx);
            panel.remove(id, cx);
            assert!(panel.tabs.is_empty());
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
            let Content::File(file) = &panel.tabs[0].content else {
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
        .update(&mut visual, |panel, _, _| assert!(panel.tabs.is_empty()))
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
            assert!(panel.tabs.is_empty());
            let tree = panel.files.clone().unwrap();
            let path = std::env::temp_dir().join("cydonia-tree-selection.md");
            tree.update(cx, |_, cx| cx.emit(super::super::files::Open(path.clone())));
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |panel, _, cx| {
            assert_eq!(panel.tabs.len(), 1);
            assert!(panel.files_open);
            let Content::File(file) = &panel.tabs[0].content else {
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
            assert_eq!(panel.tabs.len(), 1);
            assert_eq!(panel.active, Some(0));
        })
        .unwrap();
    visual.simulate_keystrokes("cmd-w");
    window
        .update(&mut visual, |panel, window, _| {
            assert!(panel.tabs.is_empty());
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
            assert_eq!(panel.tabs.len(), 2);
            let tab = panel
                .tabs
                .iter()
                .find(|tab| Some(tab.id) == panel.active)
                .unwrap();
            let Content::Terminal(terminal) = &tab.content else {
                panic!("terminal tab")
            };
            assert!(terminal.focus_handle(cx).is_focused(window));
        })
        .unwrap();
}
