mod types;

use jiff::{
    SignedDuration, ZonedDateTime,
    civil::{self, Date, Time},
    tz::TimeZone,
};

pub use types::{Seed, TimingError};

pub fn parse_duration(text: &str) -> Result<SignedDuration, TimingError> {
    let duration: SignedDuration = text
        .trim()
        .parse()
        .map_err(|_| TimingError::Invalid(text.to_string()))?;
    if duration <= SignedDuration::ZERO {
        return Err(TimingError::NotPositive(text.to_string()));
    }
    Ok(duration)
}

pub fn parse_clock(text: &str) -> Result<civil::Time, TimingError> {
    Time::strptime("%H:%M", text).map_err(|_| TimingError::Invalid(text.to_string()))
}

pub fn logical_date(now: &ZonedDateTime, day_starts_at: Time) -> Date {
    let today = now.date();
    if now.time() >= day_starts_at {
        return today;
    }
    today.yesterday().unwrap_or(today)
}

pub fn instant_on(date: Date, at: Time, day_starts_at: Time, tz: &TimeZone) -> ZonedDateTime {
    let civil_date = if at < day_starts_at {
        date.tomorrow().unwrap_or(date)
    } else {
        date
    };
    civil_date
        .at(at.hour(), at.minute(), at.second(), at.subsec_nanosecond())
        .to_zoned_datetime(tz.clone())
        .expect("civil datetime is within the supported range")
}

pub fn next_after(after: &ZonedDateTime, at: Time) -> ZonedDateTime {
    let tz = after.time_zone();
    let today = instant_on(after.date(), at, Time::midnight(), tz);
    if today.timestamp() > after.timestamp() {
        return today;
    }
    let tomorrow = after
        .date()
        .tomorrow()
        .expect("date is within the supported range");
    instant_on(tomorrow, at, Time::midnight(), tz)
}

pub fn jitter(seed: Seed, key: &str, date: Date, window: SignedDuration) -> SignedDuration {
    let mut bytes = seed.0.to_le_bytes().to_vec();
    bytes.extend_from_slice(key.as_bytes());
    bytes.extend_from_slice(date.to_string().as_bytes());
    let mut rng = fastrand::Rng::with_seed(fnv1a64(&bytes));
    let minutes = window.as_secs().max(0) / 60;
    SignedDuration::from_mins(rng.i64(0..=minutes))
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
