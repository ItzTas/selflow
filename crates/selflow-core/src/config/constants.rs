use jiff::{
    SignedDuration,
    civil::{Time, Weekday, time},
};

use super::types::Challenges;

pub const SUPPORTED_LANGUAGES: [&str; 2] = ["en", "pt-BR"];

pub const DEFAULT_LANGUAGE: &str = "en";
pub const DEFAULT_DAY_STARTS_AT: Time = time(5, 0, 0, 0);
pub const DEFAULT_LOOSENING_DELAY: SignedDuration = SignedDuration::from_hours(24);
pub const LONGEST_RULE: SignedDuration = SignedDuration::from_hours(24);
pub const DEFAULT_CHALLENGES: Challenges = Challenges {
    wait: SignedDuration::from_mins(2),
    reason: true,
    phrase: None,
    extensions: 1,
    extension_length: SignedDuration::from_mins(20),
};
pub const DEFAULT_NOTICES: [SignedDuration; 3] = [
    SignedDuration::from_mins(15),
    SignedDuration::from_mins(5),
    SignedDuration::from_mins(1),
];
pub const DEFAULT_NOTICE_AT_START: [SignedDuration; 1] = [SignedDuration::ZERO];

pub const SHUTDOWN_REPEATS: [(&str, bool); 2] = [("once", false), ("repeat", true)];

pub const DAYS: [(&str, Weekday); 7] = [
    ("mon", Weekday::Monday),
    ("tue", Weekday::Tuesday),
    ("wed", Weekday::Wednesday),
    ("thu", Weekday::Thursday),
    ("fri", Weekday::Friday),
    ("sat", Weekday::Saturday),
    ("sun", Weekday::Sunday),
];
