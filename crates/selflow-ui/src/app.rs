use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;
use selflow_platform::paths::socket_path;

use crate::lock::{LockError, backend, run_preview};

#[derive(Debug, Parser)]
#[command(name = "selflow-ui", arg_required_else_help = true)]
pub struct Cli {
    #[arg(long, conflicts_with = "preview")]
    pub lock: bool,
    #[arg(long)]
    pub preview: bool,
}

pub fn run() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .with_ansi(std::io::stderr().is_terminal())
        .try_init();
    let cli = Cli::parse();
    let result = if cli.lock {
        session_lock()
    } else {
        run_preview()
    };
    let Err(error) = result else {
        return ExitCode::SUCCESS;
    };
    tracing::error!(%error, "the lock screen failed");
    ExitCode::FAILURE
}

fn session_lock() -> Result<(), LockError> {
    let socket = socket_path().ok_or(LockError::NoRuntimeDir)?;
    backend::run_session_lock(socket)?;
    Ok(())
}
