//! How the palette cuts a match's line, reads a chord and lists commands.

use super::*;

#[test]
fn a_short_line_is_kept_whole_with_the_match_where_it_was() {
    let (text, at) = clipped("  the pump runs", 6..10);
    assert_eq!(text.as_ref(), "the pump runs");
    assert_eq!(&text[at], "pump");
}

#[test]
fn a_long_line_is_cut_ahead_of_the_match_and_says_so() {
    let line = format!("{}needle and after", "word ".repeat(40));
    let start = line.find("needle").unwrap();
    let (text, at) = clipped(&line, start..start + 6);
    assert!(text.starts_with('…'), "{text}");
    assert_eq!(&text[at], "needle");
}

#[test]
fn a_query_reads_as_a_chord_however_it_is_written() {
    let bound = menubar::chord_keys(
        &gpui::Modifiers {
            control: true,
            shift: true,
            ..Default::default()
        },
        "f",
    );
    for query in ["ctrl+shift+f", "Shift Ctrl F", "control-shift-f", "⌃⇧F"] {
        assert_eq!(menubar::query_keys(query), Some(bound.clone()), "{query}");
    }
}

#[test]
fn a_word_or_a_bare_modifier_is_not_a_chord() {
    assert_eq!(menubar::query_keys("find"), None);
    assert_eq!(menubar::query_keys("ctrl"), None);
    assert_eq!(menubar::query_keys("ctrl shift"), None);
}

#[gpui::test]
fn applying_search_hands_the_draft_to_the_workspace(cx: &mut gpui::TestAppContext) {
    use crate::model::{settings::Settings, state::State};
    use bezel::theme::Appearance;
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window =
        cx.add_window(|window, cx| Cydonia::new(Settings::default(), State::default(), window, cx));
    let root = window.root(cx).unwrap();
    let palette = window
        .update(cx, |this, window, cx| {
            let workspace = this.workspace.clone();
            let root = cx.entity().downgrade();
            cx.new(|cx| Palette::new(workspace, root, Vec::new(), window, cx))
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        palette.update(cx, |palette, cx| {
            palette
                .field
                .update(cx, |field, cx| field.set_content("retry", cx));
            palette.filter = Some(Filter::Kind(Kind::Article));
            palette.apply(&ApplySearch, window, cx);
        });
    })
    .unwrap();
    root.read_with(cx, |this, _| {
        assert_eq!(this.applied_query().unwrap().text(), "retry");
        assert_eq!(
            this.applied_draft(),
            ("retry".to_owned(), Some(Kind::Article))
        );
    });
}

#[gpui::test]
fn commands_run_last_lead_and_a_prefix_lists_commands_alone(cx: &mut gpui::TestAppContext) {
    use crate::model::{settings::Settings, state::State};
    use bezel::theme::Appearance;
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let command = |name: &str| menubar::Command {
        name: name.to_owned().into(),
        menu: "File".into(),
        action: Box::new(ToggleSearch),
        shortcut: None,
        keys: None,
    };
    let state = State {
        commands: vec!["File › Close".into(), "File › Gone".into()],
        ..State::default()
    };
    let window = cx.add_window(|window, cx| Cydonia::new(Settings::default(), state, window, cx));
    window
        .update(cx, |this, window, cx| {
            let workspace = this.workspace.clone();
            let root = cx.entity().downgrade();
            let palette = cx.new(|cx| Palette::new(workspace, root, Vec::new(), window, cx));
            palette.update(cx, |palette, cx| {
                palette.commands = vec![command("Open"), command("Close"), command("Save")];
                let names = |palette: &Palette, cx: &App| -> Vec<String> {
                    palette
                        .shown(cx)
                        .iter()
                        .filter_map(|pick| match pick {
                            Pick::Command(command) => Some(command.name.to_string()),
                            Pick::Hit(_) => None,
                        })
                        .collect()
                };
                palette.match_commands(cx);
                assert_eq!(names(palette, cx), ["Close"]);
                palette
                    .field
                    .update(cx, |field, cx| field.set_content(">", cx));
                palette.match_commands(cx);
                assert!(palette.query(cx).is_none());
                assert_eq!(names(palette, cx), ["Close", "Open", "Save"]);
                assert!(
                    palette
                        .shown(cx)
                        .iter()
                        .all(|pick| matches!(pick, Pick::Command(_)))
                );
                palette
                    .field
                    .update(cx, |field, cx| field.set_content("> sa", cx));
                palette.match_commands(cx);
                assert_eq!(names(palette, cx), ["Save"]);
            });
        })
        .unwrap();
}
