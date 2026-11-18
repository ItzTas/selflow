pub mod commands;
mod types;

use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;

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
        Command::Report(args) => commands::report_cmd(args),
        Command::Lock(args) => commands::lock_cmd(args),
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

fn error_text(localizer: &Localizer, error: &CliError) -> String {
    let id = match error {
        CliError::DaemonUnreachable => "cli-daemon-unreachable",
        CliError::NoRuntimeDir => "cli-no-runtime-dir",
        CliError::Protocol(_) => "cli-protocol-error",
        CliError::Editor(_) => "cli-editor-failed",
        CliError::UnexpectedResponse => "cli-unexpected-response",
        CliError::Lock(_) => "lock-failed",
        CliError::Daemon(message) => return message_text(localizer, message),
    };
    localizer.text(id, &[])
}

fn error_code(error: &CliError) -> u8 {
    match error {
        CliError::DaemonUnreachable | CliError::NoRuntimeDir => 2,
        _ => 1,
    }
}
