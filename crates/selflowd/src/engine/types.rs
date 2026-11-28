use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use jiff::Timestamp;
use selflow_core::config::{Config, ConfigError};
use selflow_core::plan::RuleWindow;
use selflow_protocol::ipc::LockView;
use thiserror::Error;

use crate::agent::Directives;
use crate::store::{Event, StoreError};

pub trait Enforcer: Send {
    fn warn(&mut self, title: String, body: String);
    fn show_lock(&mut self, view: LockView);
    fn hide_lock(&mut self);
    fn shut_down(&mut self);
    fn direct_agents(&mut self, directives: Directives);
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("stored configuration version {id} no longer parses: {errors:?}")]
    StoredConfig { id: i64, errors: Vec<ConfigError> },
}

#[derive(Clone, Default)]
pub struct ParsedVersions(Arc<Mutex<HashMap<i64, Arc<Config>>>>);

impl ParsedVersions {
    pub(super) fn get(&self, id: i64) -> Option<Arc<Config>> {
        self.entries().get(&id).map(Arc::clone)
    }

    pub(super) fn keep(&self, id: i64, config: Config) -> Arc<Config> {
        Arc::clone(self.entries().entry(id).or_insert_with(|| Arc::new(config)))
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<i64, Arc<Config>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
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
