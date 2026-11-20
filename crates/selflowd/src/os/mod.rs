#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{StopSignal, power_off};
