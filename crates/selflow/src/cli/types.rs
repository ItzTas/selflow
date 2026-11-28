use clap::{Args, Parser, Subcommand};
use jiff::civil;
use selflow_client::client::ClientError;
use selflow_protocol::ipc::MessageView;
use std::io;

use super::commands::{ClaudeArgs, ExceptionArgs, TestLockArgs, UnlockArgs};
use crate::claude::ClaudeError;

#[derive(Debug, Parser)]
#[command(name = "selflow", arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Claude(ClaudeArgs),
    Config(ConfigArgs),
    Exception(ExceptionArgs),
    Report(ReportArgs),
    TestLock(TestLockArgs),
    Unlock(UnlockArgs),
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    Edit,
    Status,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    #[arg(long)]
    pub date: Option<civil::Date>,
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("the daemon is unreachable")]
    DaemonUnreachable,
    #[error("no runtime directory is set")]
    NoRuntimeDir,
    #[error("talking to the daemon failed: {0}")]
    Protocol(#[source] io::Error),
    #[error("the editor failed: {0}")]
    Editor(#[source] io::Error),
    #[error("the daemon sent an unexpected answer")]
    UnexpectedResponse,
    #[error("the daemon reported an error")]
    Daemon(MessageView),
    #[error("the Selflow programs don't match")]
    VersionMismatch {
        running: Option<u32>,
        installed: Option<u32>,
    },
    #[error(transparent)]
    Claude(#[from] ClaudeError),
}

impl From<ClientError> for CliError {
    fn from(error: ClientError) -> Self {
        match error {
            ClientError::Unreachable => CliError::DaemonUnreachable,
            ClientError::Protocol(source) => CliError::Protocol(source),
            ClientError::VersionMismatch { running, installed } => {
                CliError::VersionMismatch { running, installed }
            }
        }
    }
}
