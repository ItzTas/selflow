use std::collections::HashSet;

use jiff::{
    SignedDuration,
    civil::{Time, Weekday},
};
use serde::{Deserialize, Serialize};

use super::constants::{DEFAULT_DAY_STARTS_AT, DEFAULT_LANGUAGE, DEFAULT_LOOSENING_DELAY};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub language: String,
    pub day_starts_at: Time,
    pub loosening_delay: SignedDuration,
    pub rule_sets: Vec<RuleSet>,
}

impl Config {
    pub fn empty() -> Config {
        Config {
            language: DEFAULT_LANGUAGE.to_owned(),
            day_starts_at: DEFAULT_DAY_STARTS_AT,
            loosening_delay: DEFAULT_LOOSENING_DELAY,
            rule_sets: Vec::new(),
        }
    }

    pub fn rule_set_for(&self, day: Weekday) -> Option<&RuleSet> {
        self.rule_sets.iter().find(|set| set.days.contains(&day))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    pub name: String,
    pub days: HashSet<Weekday>,
    pub challenges: Challenges,
    pub locks: Vec<LockRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Challenges {
    pub wait: SignedDuration,
    pub reason: bool,
    pub phrase: Option<String>,
    pub extensions: u32,
    pub extension_length: SignedDuration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockRule {
    pub id: String,
    pub label: String,
    pub start: Time,
    pub start_jitter: SignedDuration,
    pub end: Time,
    pub warnings: Vec<SignedDuration>,
    pub shutdown_after: Option<SignedDuration>,
}
