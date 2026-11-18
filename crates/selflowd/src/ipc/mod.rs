mod handlers;
mod types;

use std::fs::{self, DirBuilder, Permissions};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;

use selflow_protocol::ipc::{LockView, Request, read_message, write_message};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;

pub use handlers::handle;
use handlers::lock_state;
pub use types::{IpcError, Shared};

pub fn bind(path: &Path) -> Result<UnixListener, IpcError> {
    if let Some(parent) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        fs::set_permissions(parent, Permissions::from_mode(0o700))?;
    }
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        return Err(IpcError::AlreadyRunning);
    }
    if let Err(error) = fs::remove_file(path)
        && error.kind() != ErrorKind::NotFound
    {
        return Err(error.into());
    }
    UnixListener::bind(path).map_err(|error| match error.kind() {
        ErrorKind::AddrInUse => IpcError::AlreadyRunning,
        _ => error.into(),
    })
}

pub async fn serve(listener: UnixListener, shared: Arc<Shared>) -> Result<(), IpcError> {
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(converse(BufReader::new(stream), Arc::clone(&shared)));
    }
}

async fn converse(mut stream: BufReader<UnixStream>, shared: Arc<Shared>) {
    loop {
        let request = match read_message::<Request>(&mut stream).await {
            Ok(Some(request)) => request,
            Ok(None) => return,
            Err(error) => {
                tracing::debug!(%error, "closing a connection after a bad read");
                return;
            }
        };
        if request == Request::SubscribeLock {
            follow(stream, shared.views.clone()).await;
            return;
        }
        let response = handle(request, &shared);
        if let Err(error) = write_message(stream.get_mut(), &response).await {
            tracing::debug!(%error, "couldn't answer a client");
            return;
        }
    }
}

async fn follow(mut stream: BufReader<UnixStream>, mut views: watch::Receiver<Option<LockView>>) {
    let mut probe = [0; 1];
    loop {
        let state = lock_state(&views.borrow_and_update());
        if let Err(error) = write_message(stream.get_mut(), &state).await {
            tracing::debug!(%error, "a lock subscriber went away");
            return;
        }
        let open = tokio::select! {
            changed = views.changed() => changed.is_ok(),
            _ = stream.read(&mut probe) => false,
        };
        if !open {
            return;
        }
    }
}
