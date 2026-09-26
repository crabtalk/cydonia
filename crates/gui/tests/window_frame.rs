//! The main window's frame in `state.toml`.

use cydonia_gui::model::state::{Frame, Mode, State};

#[test]
fn a_frame_round_trips_through_the_state_file() {
    let state = State {
        window: Some(Frame {
            x: 120.,
            y: 48.,
            width: 1280.,
            height: 800.,
            mode: Mode::Maximized,
        }),
        ..State::default()
    };
    let body = toml::to_string_pretty(&state).unwrap();
    let back: State = toml::from_str(&body).unwrap();
    assert_eq!(back.window, state.window);
}

#[test]
fn a_state_file_without_a_frame_reads_as_none() {
    let back: State = toml::from_str("active = 0\n").unwrap();
    assert_eq!(back.window, None);
}
