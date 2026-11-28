use jiff::Timestamp;
use selflow_core::config::ConfigError;
use selflow_core::plan::RuleWindow;
use selflow_protocol::ipc::LockView;
use thiserror::Error;

use crate::store::{Event, StoreError};

pub trait Enforcer: Send {
    fn warn(&mut self, title: String, body: String);
    fn show_lock(&mut self, view: LockView);
    fn hide_lock(&mut self);
    fn shut_down(&mut self);
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("stored configuration version {id} no longer parses: {errors:?}")]
    StoredConfig { id: i64, errors: Vec<ConfigError> },
}

#[derive(Debug)]
#[expect(clippy::large_enum_variant)]
pub(super) enum Record {
    Started(RuleWindow),
    Event(Event),
}

pub(super) struct Outcome {
    pub(super) records: Vec<Record>,
    pub(super) pending_shutdown: Option<Timestamp>,
    pub(super) shut_down: bool,
}
