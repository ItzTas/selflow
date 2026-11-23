use jiff::SignedDuration;

pub const TEST_LOCK_MIN: SignedDuration = SignedDuration::from_mins(1);
pub const TEST_LOCK_MAX: SignedDuration = SignedDuration::from_hours(2);
pub const EXCEPTION_START_TTL: SignedDuration = SignedDuration::from_mins(10);
