use std::sync::Arc;

use jiff::{
    Timestamp,
    civil::{self, Weekday},
};

use crate::config::{Config, day_tag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Tighten,
    Loosen,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Change {
    pub(super) id: &'static str,
    pub(super) args: Vec<(&'static str, String)>,
    pub(super) kind: ChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Setting,
    Rule { day: Weekday, rule: String },
    Exception(civil::Date),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitChange {
    pub scope: Scope,
    pub field: &'static str,
    pub id: &'static str,
    pub args: Vec<(&'static str, String)>,
    pub kind: ChangeKind,
}

impl UnitChange {
    pub fn unit(&self) -> String {
        match &self.scope {
            Scope::Setting => self.field.to_owned(),
            Scope::Rule { day, rule } => rule_unit(*day, rule, self.field),
            Scope::Exception(date) => exception_unit(*date),
        }
    }
}

pub(super) fn rule_unit(day: Weekday, rule: &str, field: &str) -> String {
    format!("{}/{rule}/{field}", day_tag(day))
}

pub(super) fn exception_unit(date: civil::Date) -> String {
    format!("exception/{date}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVersion {
    pub id: i64,
    pub saved_at: Timestamp,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedConfig {
    pub id: i64,
    pub saved_at: Timestamp,
    pub config: Arc<Config>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredChange {
    pub version_id: i64,
    pub unit: String,
    pub kind: ChangeKind,
    pub effective_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    pub saved: Vec<SavedConfig>,
    pub changes: Vec<StoredChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedChange {
    pub change: UnitChange,
    pub effective_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownChange {
    pub id: &'static str,
    pub args: Vec<(&'static str, String)>,
    pub effective_at: Timestamp,
}
