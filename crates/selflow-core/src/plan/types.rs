use jiff::{SignedDuration, ZonedDateTime, civil};
use serde::{Deserialize, Serialize};

use crate::config::{Action, AgentStep, Challenges};

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
    #[serde(default)]
    pub agent: Vec<AgentStep>,
}

impl RuleWindow {
    pub fn key(&self) -> String {
        self.rule_id.clone()
    }

    pub fn enforces(&self) -> bool {
        self.action != Action::Nothing || !self.agent.is_empty()
    }
}
