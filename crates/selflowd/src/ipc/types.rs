use std::sync::{Arc, Mutex};

use selflow_i18n::language::Language;
use selflow_protocol::ipc::LockView;
use thiserror::Error;
use tokio::sync::{mpsc, watch};

use crate::clock::Clock;
use crate::enforcer::PowerMode;
use crate::engine::EngineError;
use crate::store::{Store, StoreError};

pub struct Shared {
    pub store: Arc<Mutex<Store>>,
    pub clock: Arc<dyn Clock>,
    pub wake: mpsc::Sender<()>,
    pub views: watch::Receiver<Option<LockView>>,
    pub power: PowerMode,
    pub env_language: Language,
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
