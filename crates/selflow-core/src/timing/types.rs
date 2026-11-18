use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed(pub u64);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TimingError {
    #[error("invalid time value: {0}")]
    Invalid(String),
    #[error("duration must be positive: {0}")]
    NotPositive(String),
}
