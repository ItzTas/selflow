mod codec;
mod constants;
mod handshake;
mod types;

pub use codec::{MessageReader, message_reader, read_message, write_message};
pub use constants::{
    HANDSHAKE_TIMEOUT, HELLO_MAGIC, MAX_FRAME_BYTES, PROTOCOL_VERSION, PROTOCOL_VERSION_FLAG,
};
pub use handshake::{hello_as_client, hello_as_daemon};
pub use types::{
    AgentKind, AgentSession, ChallengeView, ChangeView, ConfigStatusView, ExtensionSource,
    LockView, MessageView, PositionedMessage, RefusalView, ReportEntry, ReportView, Request,
    Response, StatusView, ToolCall, ToolVerdict,
};
