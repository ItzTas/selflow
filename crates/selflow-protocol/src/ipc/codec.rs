use std::ffi::OsStr;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_LINE_BYTES: usize = 1 << 20;

pub async fn write_message<T: Serialize>(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &T,
) -> io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    writer.flush().await
}

pub async fn read_message<T: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> io::Result<Option<T>> {
    let mut line = Vec::new();
    let limit = MAX_LINE_BYTES as u64 + 1;
    let read = (&mut *reader)
        .take(limit)
        .read_until(b'\n', &mut line)
        .await?;
    if read == 0 {
        return Ok(None);
    }
    if line.last() == Some(&b'\n') {
        line.pop();
    } else if line.len() > MAX_LINE_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "message line exceeds the size limit",
        ));
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

pub fn socket_path_from(runtime_dir: Option<&OsStr>) -> Option<PathBuf> {
    runtime_dir.map(|dir| Path::new(dir).join("selflow").join("daemon.sock"))
}

pub fn socket_path() -> Option<PathBuf> {
    socket_path_from(std::env::var_os("XDG_RUNTIME_DIR").as_deref())
}
