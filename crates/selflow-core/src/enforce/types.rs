use jiff::{Timestamp, civil};

use crate::plan::LockWindow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantedExtension {
    pub rule_set: String,
    pub rule_id: String,
    pub logical_date: civil::Date,
    pub granted_at: Timestamp,
    pub until: Timestamp,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NightLog {
    pub extensions: Vec<GrantedExtension>,
    pub started: Vec<LockWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::large_enum_variant)]
pub enum Enforcement {
    Free,
    Locked(ActiveLock),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveLock {
    pub window: LockWindow,
    pub locked_since: Timestamp,
    pub extension_available_at: Timestamp,
    pub extensions_left: u32,
    pub shutdown_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub key: String,
    pub label: String,
    pub at: Timestamp,
    pub lock_at: Timestamp,
    pub variant: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub enforcement: Enforcement,
    pub warnings: Vec<Warning>,
    pub next_change_at: Option<Timestamp>,
}
