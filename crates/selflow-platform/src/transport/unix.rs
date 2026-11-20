use std::io::{self, ErrorKind};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{UnixListener, UnixStream};

use super::ListenError;
use crate::secure_fs::private_dir;

pub struct Listener {
    inner: UnixListener,
}

pub struct Stream {
    inner: UnixStream,
}

impl Listener {
    pub async fn accept(&self) -> io::Result<Stream> {
        let (inner, _) = self.inner.accept().await?;
        Ok(Stream { inner })
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub fn listen(path: &Path) -> Result<Listener, ListenError> {
    if let Some(parent) = path.parent() {
        private_dir(parent)?;
    }
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        return Err(ListenError::AlreadyRunning);
    }
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != ErrorKind::NotFound
    {
        return Err(error.into());
    }
    match UnixListener::bind(path) {
        Ok(inner) => Ok(Listener { inner }),
        Err(error) if error.kind() == ErrorKind::AddrInUse => Err(ListenError::AlreadyRunning),
        Err(error) => Err(error.into()),
    }
}

pub async fn connect(path: &Path) -> io::Result<Stream> {
    Ok(Stream {
        inner: UnixStream::connect(path).await?,
    })
}
