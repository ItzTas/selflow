use jiff::{
    SignedDuration,
    civil::{Time, Weekday, time},
};

pub const SUPPORTED_LANGUAGES: [&str; 2] = ["en", "pt-BR"];

pub const DEFAULT_LANGUAGE: &str = "en";
pub const DEFAULT_DAY_STARTS_AT: Time = time(5, 0, 0, 0);
pub const DEFAULT_LOOSENING_DELAY: SignedDuration = SignedDuration::from_hours(24);
pub const DEFAULT_WAIT: SignedDuration = SignedDuration::from_mins(2);
pub const DEFAULT_REASON: bool = true;
pub const DEFAULT_EXTENSIONS: u32 = 1;
pub const DEFAULT_EXTENSION_LENGTH: SignedDuration = SignedDuration::from_mins(20);
pub const DEFAULT_WARNINGS: [SignedDuration; 3] = [
    SignedDuration::from_mins(15),
    SignedDuration::from_mins(5),
    SignedDuration::from_mins(1),
];

pub const DAYS: [(&str, Weekday); 7] = [
    ("mon", Weekday::Monday),
    ("tue", Weekday::Tuesday),
    ("wed", Weekday::Wednesday),
    ("thu", Weekday::Thursday),
    ("fri", Weekday::Friday),
    ("sat", Weekday::Saturday),
    ("sun", Weekday::Sunday),
];
