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
    #[serde(default)]
    pub exception: Vec<RawException>,
    pub agent: Option<RawAgent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawAgent {
    pub allowed_commands: Option<Vec<Spanned<String>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawAgentStep {
    pub before: Spanned<String>,
    pub level: Spanned<String>,
    pub message: Option<Spanned<toml::Value>>,
    pub folders: Option<Vec<Spanned<String>>>,
    pub unattended: Option<Spanned<bool>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRuleSet {
    pub name: Spanned<String>,
    pub days: Vec<Spanned<String>>,
    #[serde(default)]
    pub challenges: RawChallenges,
    #[serde(default)]
    pub rule: Vec<RawRule>,
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
pub struct RawRule {
    pub id: Spanned<String>,
    pub label: Option<Spanned<String>>,
    pub start: Spanned<String>,
    pub start_jitter: Option<Spanned<String>>,
    pub end: Option<Spanned<String>>,
    pub length: Option<Spanned<String>>,
    pub notices: Option<Vec<Spanned<String>>>,
    pub message: Option<Spanned<toml::Value>>,
    pub lock: Option<Spanned<bool>>,
    pub shutdown_after: Option<Spanned<String>>,
    pub shutdown: Option<Spanned<String>>,
    #[serde(default)]
    pub challenges: RawChallenges,
    #[serde(default)]
    pub agent: Vec<RawAgentStep>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawException {
    pub date: Spanned<String>,
    #[serde(default)]
    pub rules: Vec<Spanned<String>>,
}
