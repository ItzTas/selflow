use std::time::Duration;

pub const MAX_FRAME_BYTES: usize = 1 << 20;
pub const PROTOCOL_VERSION: u32 = 1;
pub const HELLO_MAGIC: [u8; 4] = *b"SLFW";
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const HELLO_BYTES: usize = HELLO_MAGIC.len() + size_of::<u32>();
