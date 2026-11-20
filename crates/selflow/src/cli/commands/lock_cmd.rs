use std::process::ExitCode;

use clap::Args;
use selflow_platform::paths::socket_path;

use crate::cli::CliError;
use crate::lock::{run_preview, run_session_lock};

#[derive(Debug, Args)]
pub struct LockArgs {
    #[arg(long)]
    pub preview: bool,
}

pub fn lock_cmd(args: LockArgs) -> Result<ExitCode, CliError> {
    if args.preview {
        run_preview()?;
        return Ok(ExitCode::SUCCESS);
    }
    let socket = socket_path().ok_or(CliError::NoRuntimeDir)?;
    run_session_lock(socket)?;
    Ok(ExitCode::SUCCESS)
}
