//! Where canvases and designs were last looked at, in `state.toml`.

use artifact::kind::Kind;
use cydonia_gui::model::state::{Camera, Entry, State};
use std::path::PathBuf;

#[test]
fn cameras_round_trip_through_the_state_file() {
    let camera = Camera {
        kind: Kind::Design,
        id: "1764000000000".into(),
        x: 1520.5,
        y: 640.,
        zoom: 0.25,
    };
    let state = State {
        last: [(
            PathBuf::from("/work/site"),
            Entry {
                kind: Kind::Design,
                id: camera.id.clone(),
            },
        )]
        .into(),
        cameras: [(PathBuf::from("/work/site"), vec![camera])].into(),
        ..State::default()
    };
    let body = toml::to_string_pretty(&state).unwrap();
    let back: State = toml::from_str(&body).unwrap();
    assert_eq!(back.cameras, state.cameras);
    assert_eq!(back.last, state.last);
}

#[test]
fn a_state_file_without_cameras_reads_as_none() {
    let back: State = toml::from_str("active = 0\n").unwrap();
    assert!(back.cameras.is_empty());
}
