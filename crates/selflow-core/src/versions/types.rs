use jiff::{
    Timestamp,
    civil::{self, Weekday},
};

use crate::config::day_tag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Tighten,
    Loosen,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub id: &'static str,
    pub args: Vec<(&'static str, String)>,
    pub kind: ChangeKind,
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
            Scope::Rule { day, rule } => format!("{}/{rule}/{}", day_tag(*day), self.field),
            Scope::Exception(date) => format!("exception/{date}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub kind: ChangeKind,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVersion {
    pub id: i64,
    pub saved_at: Timestamp,
    pub effective_at: Timestamp,
    pub text: String,
}
