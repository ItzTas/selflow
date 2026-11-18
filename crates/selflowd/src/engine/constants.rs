use jiff::SignedDuration;

pub const MAX_SLEEP: SignedDuration = SignedDuration::from_secs(60);
pub const WARNING_GRACE: SignedDuration = SignedDuration::from_secs(120);
pub const SHUTDOWN_NOTICE: SignedDuration = SignedDuration::from_secs(60);
