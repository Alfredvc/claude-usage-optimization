use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct PriceRow {
    pub model: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_creation_5m_per_mtok: f64,
    pub cache_creation_1h_per_mtok: f64,
    pub cache_read_per_mtok: f64,
    pub effective_date: String, // ISO date YYYY-MM-DD
}

/// Seed pricing — USD per 1M tokens.
///
/// Verified against https://claude.com/pricing and
/// https://platform.claude.com/docs/en/build-with-claude/prompt-caching
/// on 2026-04-17. Cache multipliers (consistent across all current models):
/// cache_5m = 1.25× input, cache_1h = 2× input, cache_read = 0.10× input.
pub fn seed_rows() -> Vec<PriceRow> {
    let date = "2026-04-17".to_string();
    let mk = |model: &str, input: f64, output: f64| PriceRow {
        model: model.to_string(),
        input_per_mtok: input,
        output_per_mtok: output,
        cache_creation_5m_per_mtok: input * 1.25,
        cache_creation_1h_per_mtok: input * 2.0,
        cache_read_per_mtok: input * 0.10,
        effective_date: date.clone(),
    };
    vec![
        mk("claude-opus-4-7", 5.00, 25.00),
        mk("claude-opus-4-6", 5.00, 25.00),
        mk("claude-sonnet-4-6", 3.00, 15.00),
        mk("claude-sonnet-4-5", 3.00, 15.00),
        mk("claude-haiku-4-5", 1.00, 5.00),
        mk("claude-3-5-sonnet-20241022", 3.00, 15.00),
        mk("claude-3-5-haiku-20241022", 0.80, 4.00),
        mk("claude-3-opus-20240229", 15.00, 75.00),
    ]
}

#[derive(Debug, Deserialize)]
struct PricingFile {
    #[serde(default)]
    models: Vec<TomlPriceRow>,
}

#[derive(Debug, Deserialize)]
struct TomlPriceRow {
    model: String,
    input_per_mtok: f64,
    output_per_mtok: f64,
    cache_creation_5m_per_mtok: Option<f64>,
    cache_creation_1h_per_mtok: Option<f64>,
    cache_read_per_mtok: Option<f64>,
    effective_date: Option<String>,
}

pub fn load_overrides(path: &Path) -> Result<Vec<PriceRow>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let parsed: PricingFile =
        toml::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(parsed
        .models
        .into_iter()
        .map(|t| PriceRow {
            cache_creation_5m_per_mtok: t
                .cache_creation_5m_per_mtok
                .unwrap_or(t.input_per_mtok * 1.25),
            cache_creation_1h_per_mtok: t
                .cache_creation_1h_per_mtok
                .unwrap_or(t.input_per_mtok * 2.0),
            cache_read_per_mtok: t.cache_read_per_mtok.unwrap_or(t.input_per_mtok * 0.10),
            effective_date: t.effective_date.unwrap_or_else(|| "1970-01-01".to_string()),
            model: t.model,
            input_per_mtok: t.input_per_mtok,
            output_per_mtok: t.output_per_mtok,
        })
        .collect())
}

/// Merge seed + overrides (overrides win on `model` collision).
pub fn merge(seed: Vec<PriceRow>, overrides: Vec<PriceRow>) -> Vec<PriceRow> {
    let mut map: HashMap<String, PriceRow> =
        seed.into_iter().map(|r| (r.model.clone(), r)).collect();
    for r in overrides {
        map.insert(r.model.clone(), r);
    }
    map.into_values().collect()
}

/// Look up a price row, or return None when the model is unknown.
pub fn build_lookup(rows: &[PriceRow]) -> HashMap<String, PriceRow> {
    rows.iter().map(|r| (r.model.clone(), r.clone())).collect()
}

/// Strip trailing `-YYYYMMDD` build-date suffix from model names.
/// e.g. `claude-haiku-4-5-20251001` → `claude-haiku-4-5`
fn normalize_model(model: &str) -> &str {
    // Check if the last segment is exactly 8 decimal digits (a date like 20251001)
    if let Some(pos) = model.rfind('-') {
        let suffix = &model[pos + 1..];
        if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit()) {
            return &model[..pos];
        }
    }
    model
}

/// Compute cost in USD given a model and usage breakdown.
#[allow(clippy::too_many_arguments)]
pub fn compute_cost(
    pricing: &HashMap<String, PriceRow>,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_5m: Option<u64>,
    cache_creation_1h: Option<u64>,
    cache_creation_input_tokens_total: Option<u64>,
    cache_read_input_tokens: Option<u64>,
) -> Option<f64> {
    let p = pricing
        .get(model)
        .or_else(|| pricing.get(normalize_model(model)))?;
    let (c5m, c1h) = match (cache_creation_5m, cache_creation_1h) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) => (a, 0),
        (None, Some(b)) => (0, b),
        (None, None) => (cache_creation_input_tokens_total.unwrap_or(0), 0),
    };
    let cr = cache_read_input_tokens.unwrap_or(0);
    let cost = (input_tokens as f64) * p.input_per_mtok
        + (output_tokens as f64) * p.output_per_mtok
        + (c5m as f64) * p.cache_creation_5m_per_mtok
        + (c1h as f64) * p.cache_creation_1h_per_mtok
        + (cr as f64) * p.cache_read_per_mtok;
    Some(cost / 1_000_000.0)
}
