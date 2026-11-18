use std::ffi::OsStr;
use std::fmt;
use std::process::{Command, Stdio};

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
    let spawned = Command::new("systemctl")
        .arg("poweroff")
        .stdin(Stdio::null())
        .spawn();
    if let Err(error) = spawned {
        tracing::error!(%error, "couldn't start systemctl poweroff");
    }
}
