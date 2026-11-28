use std::sync::{Arc, Mutex};

use jiff::civil;
use selflow_core::config::Config;
use selflow_core::enforce::DayLog;
use selflow_core::timing::Seed;
use selflow_i18n::language::Language;
use selflow_protocol::ipc::LockView;
use thiserror::Error;
use tokio::sync::{mpsc, watch};

use crate::agent::AgentHub;
use crate::clock::Clock;
use crate::enforcer::{PowerMode, ScreenControl};
use crate::engine::{EngineError, ParsedVersions};
use crate::store::{Store, StoreError};

pub struct Shared {
    pub store: Arc<Mutex<Store>>,
    pub clock: Arc<dyn Clock>,
    pub wake: mpsc::Sender<()>,
    pub views: watch::Receiver<Option<LockView>>,
    pub power: PowerMode,
    pub env_language: Language,
    pub screen: Arc<dyn ScreenControl>,
    pub parsed: ParsedVersions,
    pub agents: Arc<AgentHub>,
}

pub(super) struct ExceptionDay {
    pub(super) config: Config,
    pub(super) today: civil::Date,
    pub(super) log: DayLog,
    pub(super) seed: Seed,
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("socket error: {0}")]
    Io(#[from] std::io::Error),
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("time error: {0}")]
    Time(#[from] jiff::Error),
}
