use cacp::schema::PromptResponse;
use cydonia_gui::agent::spent::spent;

fn response(json: serde_json::Value) -> PromptResponse {
    serde_json::from_value(json).unwrap()
}

#[test]
fn the_protocol_usage_is_what_is_read() {
    let tokens = spent(&response(serde_json::json!({
        "stopReason": "end_turn",
        "usage": {"totalTokens": 3, "inputTokens": 1, "outputTokens": 2, "cachedReadTokens": 5},
        "_meta": {"quota": {"model_usage": [
            {"model": "x", "token_count": {"inputTokens": 99, "outputTokens": 99}}
        ]}}
    })))
    .unwrap();
    assert_eq!(
        (
            tokens.input,
            tokens.output,
            tokens.cache_read,
            tokens.cache_write
        ),
        (1, 2, 5, 0)
    );
    assert!(spent(&response(serde_json::json!({"stopReason": "end_turn"}))).is_none());
}

#[test]
fn a_codex_turn_scales_its_last_request_to_the_turn() {
    use artifact::stats::Tokens;
    use cydonia_gui::agent::spent::codex_turn;
    let last = Tokens {
        input: 10,
        output: 10,
        cache_read: 80,
        cache_write: 0,
    };
    let turn = codex_turn(last, 300);
    assert_eq!((turn.input, turn.output, turn.cache_read), (30, 30, 240));
    // One request: nothing to scale.
    assert_eq!(codex_turn(last, 100), last);
    assert_eq!(codex_turn(last, 0), last);
}
