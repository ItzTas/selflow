use std::cmp::Reverse;
use std::collections::HashSet;

use jiff::{
    SignedDuration,
    civil::{self, Time, Weekday},
};
use serde::{Deserialize, Serialize};

use super::constants::{
    DEFAULT_DAY_STARTS_AT, DEFAULT_LANGUAGE, DEFAULT_LOOSENING_DELAY, DEFAULT_NOTICE_AT_START,
    DEFAULT_NOTICES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub language: String,
    pub day_starts_at: Time,
    pub loosening_delay: SignedDuration,
    pub rule_sets: Vec<RuleSet>,
    pub exceptions: Vec<Exception>,
}

impl Config {
    pub fn empty() -> Config {
        Config {
            language: DEFAULT_LANGUAGE.to_owned(),
            day_starts_at: DEFAULT_DAY_STARTS_AT,
            loosening_delay: DEFAULT_LOOSENING_DELAY,
            rule_sets: Vec::new(),
            exceptions: Vec::new(),
        }
    }

    pub fn rule_set_for(&self, day: Weekday) -> Option<&RuleSet> {
        self.rule_sets.iter().find(|set| set.days.contains(&day))
    }

    pub fn rules_on(&self, day: Weekday) -> &[Rule] {
        self.rule_set_for(day).map_or(&[], |set| &set.rules)
    }

    pub fn exception_on(&self, date: civil::Date) -> Option<&Exception> {
        self.exceptions
            .iter()
            .find(|exception| exception.date == date)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    pub name: String,
    pub days: HashSet<Weekday>,
    pub rules: Vec<Rule>,
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
pub struct Rule {
    pub id: String,
    pub label: String,
    pub start: Time,
    pub start_jitter: SignedDuration,
    pub end: RuleEnd,
    pub notices: Option<Vec<SignedDuration>>,
    pub message: Vec<String>,
    pub action: Action,
    pub challenges: Challenges,
}

impl Rule {
    pub fn enforces(&self) -> bool {
        self.action != Action::Nothing
    }

    pub fn notice_offsets(&self) -> Vec<SignedDuration> {
        let mut offsets = self
            .notices
            .clone()
            .unwrap_or_else(|| self.default_notices());
        offsets.sort_by_key(|offset| Reverse(*offset));
        offsets
    }

    fn default_notices(&self) -> Vec<SignedDuration> {
        if self.enforces() {
            return DEFAULT_NOTICES.to_vec();
        }
        DEFAULT_NOTICE_AT_START.to_vec()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleEnd {
    At(Time),
    After(SignedDuration),
    DayEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Lock {
        shutdown_after: Option<SignedDuration>,
    },
    ShutDown {
        repeat: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    pub date: civil::Date,
    pub rules: Vec<String>,
}

impl Exception {
    pub fn covers(&self, rule_id: &str) -> bool {
        self.rules.is_empty() || self.rules.iter().any(|rule| rule == rule_id)
    }
}
