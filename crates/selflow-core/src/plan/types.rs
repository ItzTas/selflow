use jiff::{SignedDuration, ZonedDateTime, civil};
use serde::{Deserialize, Serialize};

use crate::config::Challenges;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockWindow {
    pub rule_set: String,
    pub rule_id: String,
    pub label: String,
    pub logical_date: civil::Date,
    pub starts_at: ZonedDateTime,
    pub ends_at: ZonedDateTime,
    pub warnings: Vec<SignedDuration>,
    pub shutdown_after: Option<SignedDuration>,
    pub challenges: Challenges,
}

impl LockWindow {
    pub fn key(&self) -> String {
        format!("{}/{}", self.rule_set, self.rule_id)
    }
}
