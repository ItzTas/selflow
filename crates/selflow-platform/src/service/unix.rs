use std::path::PathBuf;
use std::process::{Command, Output};

use super::{DAEMON_UNIT, ServiceError};

const SYSTEMCTL: &str = "systemctl";
const PATH_FIELD: &str = "path=";

pub fn parse_exec_start(output: &str) -> Option<PathBuf> {
    output
        .split([';', '{', '}'])
        .map(str::trim)
        .find_map(|field| field.strip_prefix(PATH_FIELD))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

pub fn daemon_binary() -> Result<PathBuf, ServiceError> {
    let output = systemctl(&["show", DAEMON_UNIT, "-p", "ExecStart", "--value"])?;
    parse_exec_start(&String::from_utf8_lossy(&output.stdout)).ok_or(ServiceError::NoUnit)
}

pub fn restart_daemon() -> Result<(), ServiceError> {
    systemctl(&["--no-block", "restart", DAEMON_UNIT])?;
    Ok(())
}

fn systemctl(args: &[&str]) -> Result<Output, ServiceError> {
    let output = Command::new(SYSTEMCTL).arg("--user").args(args).output()?;
    if !output.status.success() {
        return Err(ServiceError::Failed(output.status));
    }
    Ok(output)
}
