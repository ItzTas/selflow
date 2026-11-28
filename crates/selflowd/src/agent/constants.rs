use std::time::Duration;

pub const PROMPT_DELIVERY: &str = "prompt";
pub const PARAGRAPH_BREAK: &str = "\n\n";
pub const COMMAND_SEPARATOR: &str = ", ";
pub const MISSED_DELIVERY: &str = "missed";
pub const HOME_SYMBOL: &str = "~";
pub const INBOX_DELIVERY: &str = "inbox";
pub const INBOX_TIMEOUT: Duration = Duration::from_secs(2);
pub const INBOX_FRAME_TYPE: &str = "user";
pub const INBOX_ROLE: &str = "user";
pub const FRAME_END: u8 = b'\n';
