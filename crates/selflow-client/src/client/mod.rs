mod types;

use std::io::{self, ErrorKind};
use std::path::Path;

use selflow_platform::transport::{Stream, connect};
use selflow_protocol::ipc::{
    HANDSHAKE_TIMEOUT, MessageReader, PROTOCOL_VERSION, Request, Response, hello_as_client,
    message_reader, read_message, write_message,
};
use tokio::runtime::Builder;
use tokio::time::timeout;

pub use types::{Client, ClientError, LockFeed};

impl Client {
    pub fn connect(path: &Path) -> Result<Client, ClientError> {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(ClientError::Protocol)?;
        let stream = runtime.block_on(open(path))?;
        Ok(Client { runtime, stream })
    }

    pub fn request(&mut self, request: Request) -> Result<Response, ClientError> {
        self.runtime.block_on(ask(&mut self.stream, &request))
    }
}

impl LockFeed {
    pub async fn open(path: &Path) -> Result<LockFeed, ClientError> {
        let mut stream = open(path).await?;
        write_message(stream.get_mut(), &Request::SubscribeLock)
            .await
            .map_err(ClientError::Protocol)?;
        Ok(LockFeed { stream })
    }

    pub async fn next(&mut self) -> Result<Option<Response>, ClientError> {
        read_message(&mut self.stream)
            .await
            .map_err(ClientError::Protocol)
    }
}

pub async fn exchange(path: &Path, request: &Request) -> Result<Response, ClientError> {
    let mut stream = open(path).await?;
    ask(&mut stream, request).await
}

async fn open(path: &Path) -> Result<MessageReader<Stream>, ClientError> {
    let mut stream = connect(path).await.map_err(|_| ClientError::Unreachable)?;
    let running = match timeout(HANDSHAKE_TIMEOUT, hello_as_client(&mut stream)).await {
        Ok(Ok(version)) => Some(version),
        Ok(Err(error)) if is_not_a_hello(&error) => None,
        Ok(Err(error)) => return Err(ClientError::Protocol(error)),
        Err(_) => None,
    };
    if running != Some(PROTOCOL_VERSION) {
        return Err(ClientError::VersionMismatch {
            running,
            installed: None,
        });
    }
    Ok(message_reader(stream))
}

fn is_not_a_hello(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::InvalidData | ErrorKind::UnexpectedEof
    )
}

async fn ask(
    stream: &mut MessageReader<Stream>,
    request: &Request,
) -> Result<Response, ClientError> {
    write_message(stream.get_mut(), request)
        .await
        .map_err(ClientError::Protocol)?;
    read_message(stream)
        .await
        .map_err(ClientError::Protocol)?
        .ok_or(ClientError::Unreachable)
}
