use artifact::{kind::Kind, space::Member};
use cydonia_gui::model::{state::State, workspace::Slot};
use std::path::PathBuf;

#[test]
fn the_library_tab_reads_back_beside_entry_and_space_tabs() {
    let state = State {
        tabs: Some(vec![
            Slot::Entry(Member::new(PathBuf::from("/repos/a"), Kind::Board, "1")),
            Slot::Space {
                space: "1792663746899".into(),
            },
            Slot::Library {},
        ]),
        front: Some(Slot::Library {}),
        ..State::default()
    };
    let written = toml::to_string(&state).unwrap();
    let read: State = toml::from_str(&written).unwrap();
    assert_eq!(read.tabs, state.tabs);
    assert_eq!(read.front, Some(Slot::Library {}));
}
