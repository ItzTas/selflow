use super::constants::{FRAME_END, INBOX_FRAME_TYPE, INBOX_ROLE};
use super::types::{InboxFrame, InboxMessage};

pub fn inbox_frame(text: &str) -> Vec<u8> {
    let frame = InboxFrame {
        kind: INBOX_FRAME_TYPE,
        message: InboxMessage {
            role: INBOX_ROLE,
            content: text,
        },
    };
    let mut bytes = serde_json::to_vec(&frame).expect("a frame of strings serializes");
    bytes.push(FRAME_END);
    bytes
}
