use jiff::{SignedDuration, ZonedDateTime, civil};
use serde::{Deserialize, Serialize};

use crate::config::{Action, Challenges};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleWindow {
    pub set: String,
    pub rule_id: String,
    pub label: String,
    pub logical_date: civil::Date,
    pub starts_at: ZonedDateTime,
    pub ends_at: ZonedDateTime,
    pub notices: Vec<SignedDuration>,
    pub message: Vec<String>,
    pub action: Action,
    pub challenges: Challenges,
    #[serde(default)]
    pub test: bool,
}

impl RuleWindow {
    pub fn key(&self) -> String {
        self.rule_id.clone()
    }
}
