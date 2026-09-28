use super::*;

fn saved(active: usize) -> SavedPanel {
    SavedPanel {
        tabs: vec![
            SavedTab::Browser {
                id: 1,
                url: "https://example.com".into(),
                title: "Example".into(),
            },
            SavedTab::Review,
            SavedTab::File {
                path: "draft.txt".into(),
                draft: Some(("original".into(), "edited".into())),
            },
            SavedTab::Browser {
                id: 2,
                url: "https://example.org".into(),
                title: "Other".into(),
            },
        ],
        active: Some(active),
        ..Default::default()
    }
}

#[test]
fn clearing_browser_tabs_preserves_file_drafts_and_selection() {
    let mut panel = saved(2);
    panel.remove_browsers();
    assert_eq!(panel.tabs.len(), 2);
    assert_eq!(panel.active, Some(1));
    assert!(
        matches!(&panel.tabs[1], SavedTab::File { draft: Some((_, text)), .. } if text == "edited")
    );
    let mut panel = saved(3);
    panel.remove_browsers();
    assert_eq!(panel.active, Some(0));
    panel.tabs = vec![SavedTab::Browser {
        id: 1,
        url: String::new(),
        title: String::new(),
    }];
    panel.remove_browsers();
    assert!(panel.tabs.is_empty());
    assert_eq!(panel.active, None);
}

#[cfg(not(target_os = "linux"))]
#[gpui::test]
fn clearing_closes_live_and_pending_browser_tabs(cx: &mut gpui::TestAppContext) {
    let panel = cx.new(|cx| Panel::new(std::env::temp_dir(), cx));
    panel.update(cx, |panel, cx| {
        panel.review(cx);
        panel.browser(1, "about:blank".into(), String::new(), cx);
        panel.browser(2, "about:blank".into(), String::new(), cx);
        panel.restore_pending = Some(saved(2));
        panel.close_browsers(cx);
        assert!(panel.browsers().is_empty());
        assert_eq!(panel.strip.len(), 1);
        let pending = panel.restore_pending.as_ref().unwrap();
        assert_eq!(pending.tabs.len(), 2);
        assert_eq!(pending.active, Some(1));
        crate::view::component::browser::set_clearing(true, cx);
        assert!(panel.open_browser("about:blank".into(), cx).is_none());
        crate::view::component::browser::set_clearing(false, cx);
    });
}

#[cfg(not(target_os = "linux"))]
#[gpui::test]
fn clearing_removes_browsers_from_background_projects_and_disk(cx: &mut gpui::TestAppContext) {
    use crate::model::{settings::Settings, state};
    let scratch =
        std::env::temp_dir().join(format!("cydonia-clear-browser-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &scratch);
    }
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Dark, cx));
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });
    let panel = cx.new(|cx| Panel::new(scratch.join("background"), cx));
    panel.update(cx, |panel, cx| {
        panel.review(cx);
        panel.browser(100, "about:blank".into(), String::new(), cx);
    });
    window
        .update(cx, |root, _, _| {
            root.right_panels
                .insert(scratch.join("background"), panel.clone());
        })
        .unwrap();
    let stored = SavedPanels {
        projects: [(scratch.join("unopened"), saved(2))].into(),
        ..Default::default()
    };
    let file = path().unwrap();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, serde_json::to_vec(&stored).unwrap()).unwrap();
    cx.update(|cx| {
        crate::view::component::browser::set_clearing(true, cx);
        super::super::close_all_browsers(cx).unwrap();
        crate::view::component::browser::set_clearing(false, cx);
    });
    panel.read_with(cx, |panel, _| {
        assert!(panel.browsers().is_empty());
        assert_eq!(panel.strip.len(), 1);
    });
    let stored = load();
    assert!(stored.projects.values().all(|panel| {
        panel
            .tabs
            .iter()
            .all(|tab| !matches!(tab, SavedTab::Browser { .. }))
    }));
    let unopened = &stored.projects[&scratch.join("unopened")];
    assert_eq!(unopened.active, Some(1));
    assert!(
        matches!(&unopened.tabs[1], SavedTab::File { draft: Some((_, text)), .. } if text == "edited")
    );
    std::fs::remove_dir_all(scratch).unwrap();
}
