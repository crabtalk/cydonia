use cydonia_gui::model::prices::Prices;

const LIST: &str = r#"{
    "sample_spec": {"mode": "chat"},
    "claude-opus-5-5": {"mode": "chat", "input_cost_per_token": 5e-6, "output_cost_per_token": 2.5e-5,
        "cache_read_input_token_cost": 5e-7, "cache_creation_input_token_cost": 6.25e-6},
    "bedrock/claude-opus-5-5": {"mode": "chat", "input_cost_per_token": 6e-6, "output_cost_per_token": 3e-5},
    "openai/gpt-x": {"mode": "chat", "input_cost_per_token": 1e-6, "output_cost_per_token": 2e-6},
    "azure/gpt-x": {"mode": "chat", "input_cost_per_token": 1e-6, "output_cost_per_token": 2e-6},
    "a/split": {"mode": "chat", "input_cost_per_token": 1e-6, "output_cost_per_token": 2e-6},
    "b/split": {"mode": "chat", "input_cost_per_token": 3e-6, "output_cost_per_token": 2e-6},
    "text-embedding": {"mode": "embedding", "input_cost_per_token": 1e-7, "output_cost_per_token": 0}
}"#;

#[test]
fn lookup_follows_full_keys_bare_names_and_suffixes() {
    let prices = Prices::parse(LIST).unwrap();
    let opus = prices.rates("Claude-Opus-5-5[1m]").unwrap();
    assert!((opus.input - 5.0).abs() < 1e-9);
    assert!((opus.cache_write - 6.25).abs() < 1e-9);
    // A full key beats the bare name of a provider-prefixed one.
    assert!((prices.rates("claude-opus-5-5").unwrap().input - 5.0).abs() < 1e-9);
    assert!((prices.rates("gpt-x").unwrap().output - 2.0).abs() < 1e-9);
    assert!(prices.rates("split").is_none());
    assert!(prices.rates("a/split").is_some());
    assert!(prices.rates("opus").is_none());
    assert!(prices.rates("<synthetic>").is_none());
    assert!(prices.rates("text-embedding").is_none());
}

#[test]
fn a_missing_cache_rate_is_the_input_rate() {
    let prices = Prices::parse(LIST).unwrap();
    let gpt = prices.rates("gpt-x").unwrap();
    assert!((gpt.cache_read - 1.0).abs() < 1e-9);
    assert!((gpt.cache_write - 1.0).abs() < 1e-9);
}
