use super::*;

fn modes(current: &str) -> SessionModeState {
    serde_json::from_value(serde_json::json!({
        "currentModeId": current,
        "availableModes": [
            { "id": "ask", "name": "Ask" },
            { "id": "code", "name": "Code" },
        ],
    }))
    .unwrap()
}

fn offers(current: &str) -> Offers {
    Offers {
        modes: Some(modes(current)),
        config: Vec::new(),
    }
}

fn choice(mode: &str) -> Choices {
    Choices {
        mode: Some(mode.into()),
        config: BTreeMap::new(),
    }
}

#[test]
fn a_saved_mode_the_agent_offers_is_on() {
    let applied = offers("ask").with(&choice("code"));
    assert_eq!(&*applied.modes.unwrap().current_mode_id, "code");
}

#[test]
fn a_saved_mode_the_agent_no_longer_offers_is_left_out() {
    let applied = offers("ask").with(&choice("gone"));
    assert_eq!(&*applied.modes.unwrap().current_mode_id, "ask");
}
