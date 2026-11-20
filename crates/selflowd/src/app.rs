use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use selflow_platform::paths::{socket_path, store_path};
use selflow_platform::transport::{ListenError, Listener, listen};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinError;

use crate::clock::SystemClock;
use crate::enforcer::{LockSupervisor, Notifier, SystemEnforcer, lock_program_from, power_mode};
use crate::engine::{Engine, EngineError};
use crate::ipc::{IpcError, Shared, serve};
use crate::os::StopSignal;
use crate::store::{Store, StoreError};

#[derive(Debug, Error)]
pub enum AppError {
    #[error("no place for the store: set XDG_STATE_HOME or HOME")]
    NoStatePath,
    #[error("no runtime directory for the socket: set XDG_RUNTIME_DIR")]
    NoRuntimeDir,
    #[error("another selflowd daemon is already running on {}", .0.display())]
    AlreadyRunning(PathBuf),
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Ipc(#[from] IpcError),
    #[error("system error: {0}")]
    Io(#[from] io::Error),
    #[error("the socket server stopped: {0}")]
    Server(#[from] JoinError),
}

pub async fn run() -> Result<(), AppError> {
    let store_path = store_path().ok_or(AppError::NoStatePath)?;
    let socket = socket_path().ok_or(AppError::NoRuntimeDir)?;
    let listener = listen(&socket).map_err(|error| match error {
        ListenError::AlreadyRunning => AppError::AlreadyRunning(socket.clone()),
        ListenError::Io(error) => AppError::Io(error),
    })?;
    let outcome = run_bound(listener, &store_path).await;
    if let Err(error) = std::fs::remove_file(&socket) {
        tracing::warn!(%error, "couldn't remove the socket");
    }
    outcome
}

async fn run_bound(listener: Listener, store_path: &Path) -> Result<(), AppError> {
    let store = Arc::new(Mutex::new(Store::open(store_path)?));
    let clock = Arc::new(SystemClock);
    let supervisor = LockSupervisor::new(
        lock_program_from(&std::env::current_exe()?),
        vec!["--lock".into()],
    );
    let power = power_mode();
    tracing::info!("power mode: {power}");
    let enforcer = SystemEnforcer::new(supervisor, Notifier, power);
    let views = enforcer.views();
    let mut engine = Engine::new(Arc::clone(&clock), enforcer, Arc::clone(&store))?;
    let mut terminate = StopSignal::new()?;
    if let Err(error) = engine.tick() {
        tracing::error!(%error, "the first enforcement tick failed");
    }
    let (wake, woken) = mpsc::channel(1);
    let shared = Arc::new(Shared {
        store,
        clock,
        wake,
        views,
        power,
    });
    let mut server = tokio::spawn(serve(listener, shared));
    tracing::info!("selflowd is running");
    tokio::select! {
        finished = engine.run(woken) => Ok(finished?),
        served = &mut server => Ok(served??),
        _ = terminate.recv() => {
            tracing::info!("stopping on SIGTERM");
            Ok(())
        }
    }
}
