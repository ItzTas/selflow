mod constants;
mod types;
#[cfg(unix)]
mod unix;

pub use constants::DAEMON_UNIT;
pub use types::ServiceError;
#[cfg(unix)]
pub use unix::{daemon_binary, parse_exec_start, restart_daemon};
