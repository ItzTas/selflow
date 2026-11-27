use std::io::{self, ErrorKind};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec, LinesCodecError};

pub const MAX_LINE_BYTES: usize = 1 << 20;

pub type MessageReader<R> = FramedRead<R, LinesCodec>;

pub fn message_reader<R: AsyncRead>(reader: R) -> MessageReader<R> {
    FramedRead::new(reader, lines())
}

pub async fn write_message<T: Serialize>(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &T,
) -> io::Result<()> {
    let line = serde_json::to_string(message)?;
    FramedWrite::new(writer, lines())
        .send(line)
        .await
        .map_err(into_io_error)
}

pub async fn read_message<T: DeserializeOwned>(
    reader: &mut MessageReader<impl AsyncRead + Unpin>,
) -> io::Result<Option<T>> {
    let Some(line) = reader.next().await.transpose().map_err(into_io_error)? else {
        return Ok(None);
    };
    serde_json::from_str(&line)
        .map(Some)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

fn lines() -> LinesCodec {
    LinesCodec::new_with_max_length(MAX_LINE_BYTES)
}

fn into_io_error(error: LinesCodecError) -> io::Error {
    match error {
        LinesCodecError::MaxLineLengthExceeded => io::Error::new(
            ErrorKind::InvalidData,
            "message line exceeds the size limit",
        ),
        LinesCodecError::Io(error) => error,
    }
}
