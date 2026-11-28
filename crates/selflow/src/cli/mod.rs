pub mod commands;
mod types;

use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::PROTOCOL_VERSION;

use crate::render::message_text;

pub use types::{Cli, CliError, Command, ConfigAction, ConfigArgs, ReportArgs};

pub fn run() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .with_ansi(std::io::stderr().is_terminal())
        .try_init();
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Config(args) => commands::config_cmd(args),
        Command::Exception(args) => commands::exception_cmd(args),
        Command::Report(args) => commands::report_cmd(args),
        Command::TestLock(args) => commands::test_lock_cmd(args),
        Command::Unlock(args) => commands::unlock_cmd(args),
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            let localizer = Localizer::new(Language::from_env());
            eprintln!("{}", error_text(&localizer, &error));
            ExitCode::from(error_code(&error))
        }
    }
}

pub fn error_text(localizer: &Localizer, error: &CliError) -> String {
    let id = match error {
        CliError::DaemonUnreachable => "cli-daemon-unreachable",
        CliError::NoRuntimeDir => "cli-no-runtime-dir",
        CliError::Protocol(_) => "cli-protocol-error",
        CliError::Editor(_) => "cli-editor-failed",
        CliError::UnexpectedResponse => "cli-unexpected-response",
        CliError::Daemon(message) => return message_text(localizer, message),
        CliError::VersionMismatch { running, installed } => {
            return localizer.text(
                "cli-version-mismatch",
                &[
                    ("client", Arg::Text(PROTOCOL_VERSION.to_string())),
                    ("running", Arg::Text(version_text(*running))),
                    ("installed", Arg::Text(version_text(*installed))),
                ],
            );
        }
    };
    localizer.text(id, &[])
}

fn version_text(version: Option<u32>) -> String {
    version.map_or_else(|| "?".to_owned(), |version| version.to_string())
}

pub fn error_code(error: &CliError) -> u8 {
    match error {
        CliError::DaemonUnreachable | CliError::NoRuntimeDir | CliError::VersionMismatch { .. } => {
            2
        }
        _ => 1,
    }
}
