use std::io::{self, ErrorKind};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::bytes::Bytes;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use super::constants::MAX_FRAME_BYTES;

pub type MessageReader<R> = FramedRead<R, LengthDelimitedCodec>;

pub fn message_reader<R: AsyncRead>(reader: R) -> MessageReader<R> {
    FramedRead::new(reader, frames())
}

pub async fn write_message<T: Serialize>(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &T,
) -> io::Result<()> {
    let payload = postcard::to_stdvec(message).map_err(invalid_data)?;
    FramedWrite::new(writer, frames())
        .send(Bytes::from(payload))
        .await
}

pub async fn read_message<T: DeserializeOwned>(
    reader: &mut MessageReader<impl AsyncRead + Unpin>,
) -> io::Result<Option<T>> {
    let Some(frame) = reader.next().await.transpose()? else {
        return Ok(None);
    };
    postcard::from_bytes(&frame).map(Some).map_err(invalid_data)
}

fn frames() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_BYTES)
        .new_codec()
}

fn invalid_data(error: postcard::Error) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, error)
}
