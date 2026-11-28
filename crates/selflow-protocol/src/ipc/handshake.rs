use std::io::{self, ErrorKind};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::constants::{HELLO_BYTES, HELLO_MAGIC, PROTOCOL_VERSION};

pub async fn hello_as_client(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
) -> io::Result<u32> {
    send_hello(stream).await?;
    read_hello(stream).await
}

pub async fn hello_as_daemon(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
) -> io::Result<u32> {
    let version = read_hello(stream).await?;
    send_hello(stream).await?;
    Ok(version)
}

async fn send_hello(stream: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
    let mut hello = [0; HELLO_BYTES];
    hello[..HELLO_MAGIC.len()].copy_from_slice(&HELLO_MAGIC);
    hello[HELLO_MAGIC.len()..].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    stream.write_all(&hello).await?;
    stream.flush().await
}

async fn read_hello(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<u32> {
    let mut hello = [0; HELLO_BYTES];
    stream.read_exact(&mut hello).await?;
    let (magic, version) = hello.split_at(HELLO_MAGIC.len());
    if magic != HELLO_MAGIC {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "the peer didn't open with a Selflow hello",
        ));
    }
    Ok(u32::from_le_bytes(
        version.try_into().expect("four version bytes"),
    ))
}
