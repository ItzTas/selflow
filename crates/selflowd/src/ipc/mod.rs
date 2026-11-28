mod constants;
mod handlers;
mod types;

use std::sync::Arc;

use selflow_platform::transport::{AcceptError, Listener, Stream};
use selflow_protocol::ipc::{
    AgentSession, HANDSHAKE_TIMEOUT, LockView, MessageReader, PROTOCOL_VERSION, Request, Response,
    hello_as_daemon, message_reader, read_message, write_message,
};
use tokio::io::AsyncReadExt;
use tokio::sync::watch;
use tokio::time::timeout;

pub use handlers::handle;
use handlers::lock_state;
pub use types::{IpcError, Shared};

pub async fn serve(listener: Listener, shared: Arc<Shared>) -> Result<(), IpcError> {
    loop {
        match listener.accept().await {
            Ok(stream) => {
                tokio::spawn(converse(stream, Arc::clone(&shared)));
            }
            Err(AcceptError::ForeignPeer { uid }) => {
                tracing::warn!(uid, "refused a connection from another user");
            }
            Err(AcceptError::Io(error)) => return Err(error.into()),
        }
    }
}

async fn converse(mut stream: Stream, shared: Arc<Shared>) {
    match timeout(HANDSHAKE_TIMEOUT, hello_as_daemon(&mut stream)).await {
        Ok(Ok(PROTOCOL_VERSION)) => {}
        Ok(Ok(version)) => {
            tracing::debug!(version, "closing a connection on another protocol version");
            return;
        }
        Ok(Err(error)) => {
            tracing::debug!(%error, "closing a connection after a bad hello");
            return;
        }
        Err(_) => {
            tracing::debug!("closing a connection that never finished its hello");
            return;
        }
    }
    let mut stream = message_reader(stream);
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
        let answered = match request {
            Request::AgentPrompt(session) => answer_prompt(&mut stream, session, &shared).await,
            request => answer(&mut stream, &handle(request, &shared)).await,
        };
        if !answered {
            return;
        }
    }
}

async fn answer(stream: &mut MessageReader<Stream>, response: &Response) -> bool {
    let Err(error) = write_message(stream.get_mut(), response).await else {
        return true;
    };
    tracing::debug!(%error, "couldn't answer a client");
    false
}

async fn answer_prompt(
    stream: &mut MessageReader<Stream>,
    session: AgentSession,
    shared: &Shared,
) -> bool {
    let prompt = shared.agents.answer_prompt(session, &shared.clock.now());
    if answer(stream, &prompt.response).await {
        shared.agents.delivered(&prompt);
        return true;
    }
    shared.agents.undelivered(&prompt);
    false
}

async fn follow(mut stream: MessageReader<Stream>, mut views: watch::Receiver<Option<LockView>>) {
    let mut probe = [0; 1];
    loop {
        let state = lock_state(&views.borrow_and_update());
        if let Err(error) = write_message(stream.get_mut(), &state).await {
            tracing::debug!(%error, "a lock subscriber went away");
            return;
        }
        let open = tokio::select! {
            changed = views.changed() => changed.is_ok(),
            _ = stream.get_mut().read(&mut probe) => false,
        };
        if !open {
            return;
        }
    }
}
