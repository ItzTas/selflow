use jiff::civil::Date;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Status,
    SubscribeLock,
    RequestExtension {
        reason: String,
        phrase: String,
    },
    GetConfig,
    SaveConfig {
        text: String,
    },
    ConfigStatus,
    Report {
        logical_date: Option<Date>,
    },
    TestLock {
        rule: Option<String>,
        duration: SignedDuration,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Status(StatusView),
    Lock(LockView),
    LockEnded,
    ExtensionGranted {
        until: Timestamp,
    },
    ExtensionRefused(RefusalView),
    Config {
        text: String,
    },
    Saved {
        saved_at: Timestamp,
        changes: Vec<ChangeView>,
    },
    Invalid {
        errors: Vec<PositionedMessage>,
    },
    ConfigStatus(ConfigStatusView),
    Report(ReportView),
    TestLockStarted {
        label: String,
        ends_at: Timestamp,
    },
    Error(MessageView),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    pub id: String,
    pub args: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionedMessage {
    pub message: MessageView,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusView {
    pub language: String,
    pub locked: Option<LockView>,
    pub next_lock_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockView {
    pub label: String,
    pub ends_at: Timestamp,
    pub extension_available_at: Timestamp,
    pub extensions_left: u32,
    pub extension_minutes: i64,
    pub reason_required: bool,
    pub phrase: Option<String>,
    pub shutdown_at: Option<Timestamp>,
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "snake_case")]
pub enum RefusalView {
    TooEarly { available_at: Timestamp },
    NoBudget,
    ReasonTooShort { min_chars: usize },
    PhraseMismatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeView {
    pub message: MessageView,
    pub effective_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigStatusView {
    pub last_saved_at: Option<Timestamp>,
    pub pending: Vec<ChangeView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportView {
    pub logical_date: Date,
    pub entries: Vec<ReportEntry>,
    pub pending: Vec<ChangeView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportEntry {
    pub at: Timestamp,
    pub message: MessageView,
}
