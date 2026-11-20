use std::ffi::OsStr;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    Real,
    LogOnly,
}

impl fmt::Display for PowerMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PowerMode::Real => "real",
            PowerMode::LogOnly => "log",
        })
    }
}

pub fn power_mode_from(value: Option<&OsStr>) -> PowerMode {
    if value == Some(OsStr::new("log")) {
        return PowerMode::LogOnly;
    }
    PowerMode::Real
}

pub fn power_mode() -> PowerMode {
    power_mode_from(std::env::var_os("SELFLOW_POWER").as_deref())
}

pub fn shut_down(mode: PowerMode) {
    if mode == PowerMode::LogOnly {
        tracing::warn!("shutdown requested");
        return;
    }
    if let Err(error) = crate::os::power_off() {
        tracing::error!(%error, "couldn't start the shutdown");
    }
}
