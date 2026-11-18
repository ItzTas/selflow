use jiff::{Timestamp, civil};
use selflow_core::config::ConfigError;
use selflow_protocol::ipc::LockView;
use thiserror::Error;

use crate::store::StoreError;

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

pub(super) struct ShownLock {
    pub(super) label: String,
    pub(super) logical_date: civil::Date,
    pub(super) first_shown: Timestamp,
    pub(super) ended: bool,
    pub(super) shut_down: bool,
}
