use std::collections::HashMap;

use serde::Deserialize;
use toml::Spanned;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    pub language: Option<Spanned<String>>,
    pub day_starts_at: Option<Spanned<String>>,
    pub loosening_delay: Option<Spanned<String>>,
    #[serde(default)]
    pub anchors: HashMap<String, Spanned<String>>,
    #[serde(default)]
    pub rule_set: Vec<RawRuleSet>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRuleSet {
    pub name: Spanned<String>,
    pub days: Vec<Spanned<String>>,
    #[serde(default)]
    pub challenges: RawChallenges,
    #[serde(default)]
    pub lock: Vec<RawLock>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawChallenges {
    pub wait: Option<Spanned<String>>,
    pub reason: Option<Spanned<bool>>,
    pub phrase: Option<Spanned<String>>,
    pub extensions: Option<Spanned<u32>>,
    pub extension_length: Option<Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLock {
    pub id: Spanned<String>,
    pub label: Option<Spanned<String>>,
    pub start: Spanned<String>,
    pub start_jitter: Option<Spanned<String>>,
    pub end: Spanned<String>,
    pub warnings: Option<Vec<Spanned<String>>>,
    pub shutdown_after: Option<Spanned<String>>,
}
