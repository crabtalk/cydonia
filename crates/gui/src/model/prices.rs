//! Per-model token prices, from LiteLLM's price list.
//!
//! The list is fetched at runtime and cached in [`settings::data_dir`]; it is
//! not built into the binary.

use crate::model::settings;
use anyhow::{Context as _, Result};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, time::Duration};

const URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

/// A cache younger than this is used without asking upstream.
const FRESH: Duration = Duration::from_secs(24 * 60 * 60);

/// Names that stand for a family rather than a model, and so name no price.
const UNPRICED: [&str; 5] = ["opus", "sonnet", "haiku", "fable", "<synthetic>"];

/// USD per million tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    /// The input rate where the price list names none.
    pub cache_read: f64,
    /// The input rate where the price list names none.
    pub cache_write: f64,
}

impl Rates {
    pub fn cost(&self, tokens: &crate::model::statistics::Tokens) -> f64 {
        (tokens.input as f64 * self.input
            + tokens.output as f64 * self.output
            + tokens.cache_read as f64 * self.cache_read
            + tokens.cache_write as f64 * self.cache_write)
            / 1_000_000.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Prices(HashMap<String, Rates>);

impl Prices {
    /// Chat models with an input and an output price. Keys are lowercased; a
    /// bare name (after the last `/`) is added when every entry carrying it
    /// agrees on rates and no full key already is that name.
    pub fn parse(body: &str) -> Result<Self> {
        let all: serde_json::Map<String, Value> = serde_json::from_str(body)?;
        let mut full = HashMap::new();
        for (key, entry) in &all {
            if entry.get("mode").and_then(Value::as_str) != Some("chat") {
                continue;
            }
            let rate = |field: &str| {
                entry
                    .get(field)
                    .and_then(Value::as_f64)
                    .map(|per_token| per_token * 1_000_000.0)
            };
            let (Some(input), Some(output)) =
                (rate("input_cost_per_token"), rate("output_cost_per_token"))
            else {
                continue;
            };
            full.insert(
                key.to_lowercase(),
                Rates {
                    input,
                    output,
                    cache_read: rate("cache_read_input_token_cost").unwrap_or(input),
                    cache_write: rate("cache_creation_input_token_cost").unwrap_or(input),
                },
            );
        }
        let mut bare: HashMap<String, Option<Rates>> = HashMap::new();
        for (key, rates) in &full {
            let Some((_, name)) = key.rsplit_once('/') else {
                continue;
            };
            bare.entry(name.to_owned())
                .and_modify(|seen| {
                    if *seen != Some(*rates) {
                        *seen = None;
                    }
                })
                .or_insert(Some(*rates));
        }
        for (name, rates) in bare {
            if let Some(rates) = rates {
                full.entry(name).or_insert(rates);
            }
        }
        Ok(Self(full))
    }

    /// The rates for a model as an agent names it. A `[...]` suffix is
    /// dropped; family names are unpriced.
    pub fn rates(&self, model: &str) -> Option<Rates> {
        let mut model = model.trim().to_lowercase();
        if let Some(open) = model.find('[')
            && model.ends_with(']')
        {
            model.truncate(open);
        }
        if UNPRICED.contains(&model.as_str()) {
            return None;
        }
        self.0.get(&model).copied().or_else(|| {
            let (_, name) = model.rsplit_once('/')?;
            self.0.get(name).copied()
        })
    }
}

fn cache() -> Result<PathBuf> {
    Ok(settings::data_dir()?.join("prices"))
}

/// The price list: the cache when it is fresh, otherwise upstream with the
/// cached ETag, falling back to the cache when upstream cannot be reached.
/// Blocking.
pub fn load() -> Result<Prices> {
    let dir = cache()?;
    let file = dir.join("model_prices.json");
    let etag_file = dir.join("etag");
    let fresh = std::fs::metadata(&file)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age < FRESH);
    if !fresh
        && let Err(err) = refresh(&file, &etag_file)
        && !file.is_file()
    {
        return Err(err);
    }
    Prices::parse(&std::fs::read_to_string(&file)?)
}

fn refresh(file: &std::path::Path, etag_file: &std::path::Path) -> Result<()> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .new_agent();
    let mut request = agent.get(URL);
    let etag = std::fs::read_to_string(etag_file).ok();
    if file.is_file()
        && let Some(etag) = &etag
    {
        request = request.header("If-None-Match", etag.trim());
    }
    let mut response = request
        .call()
        .context("the price list could not be fetched")?;
    if response.status() == 304 {
        // Unchanged: restart the freshness clock.
        let body = std::fs::read(file)?;
        std::fs::write(file, body)?;
        return Ok(());
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_string()?;
    Prices::parse(&body).context("the price list is not the shape this build reads")?;
    let dir = file.parent().context("price cache has no directory")?;
    std::fs::create_dir_all(dir)?;
    let part = file.with_extension("part");
    std::fs::write(&part, &body)?;
    std::fs::rename(&part, file)?;
    match etag {
        Some(etag) => std::fs::write(etag_file, etag)?,
        None => {
            let _ = std::fs::remove_file(etag_file);
        }
    }
    Ok(())
}
