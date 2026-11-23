use jiff::{Timestamp, civil};

use crate::plan::RuleWindow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantedExtension {
    pub rule_set: String,
    pub rule_id: String,
    pub logical_date: civil::Date,
    pub granted_at: Timestamp,
    pub until: Timestamp,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantedException {
    pub rule_id: String,
    pub logical_date: civil::Date,
    pub granted_at: Timestamp,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownDone {
    pub rule_id: String,
    pub logical_date: civil::Date,
    pub at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DayLog {
    pub extensions: Vec<GrantedExtension>,
    pub started: Vec<RuleWindow>,
    pub exceptions: Vec<GrantedException>,
    pub shutdowns: Vec<ShutdownDone>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::large_enum_variant)]
pub enum Enforcement {
    Free,
    Locked(ActiveLock),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveLock {
    pub window: RuleWindow,
    pub locked_since: Timestamp,
    pub extension_available_at: Timestamp,
    pub extensions_left: u32,
    pub shutdown_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Lock,
    ShutDown,
    Reminder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub key: String,
    pub label: String,
    pub at: Timestamp,
    pub moment: Timestamp,
    pub variant: u8,
    pub kind: NoticeKind,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub enforcement: Enforcement,
    pub notices: Vec<Notice>,
    pub next_change_at: Option<Timestamp>,
    pub shutdown: Option<RuleWindow>,
}
