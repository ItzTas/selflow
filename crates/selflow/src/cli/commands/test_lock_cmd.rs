use std::process::ExitCode;

use clap::Args;
use jiff::SignedDuration;
use jiff::tz::TimeZone;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_platform::paths::socket_path;
use selflow_protocol::ipc::{Request, Response};

use super::config_cmd::{daemon_language, unexpected};
use crate::cli::CliError;
use crate::client::Client;

#[derive(Debug, Args)]
pub struct TestLockArgs {
    #[arg(long)]
    pub rule: Option<String>,
    #[arg(long = "for", default_value = "5m")]
    pub duration: SignedDuration,
}

pub fn test_lock_cmd(args: TestLockArgs) -> Result<ExitCode, CliError> {
    let path = socket_path().ok_or(CliError::NoRuntimeDir)?;
    let mut client = Client::connect(&path)?;
    let localizer = Localizer::new(daemon_language(&mut client)?);
    let request = Request::TestLock {
        rule: args.rule,
        duration: args.duration,
    };
    let (label, ends_at) = match client.request(request)? {
        Response::TestLockStarted { label, ends_at } => (label, ends_at),
        other => return Err(unexpected(other)),
    };
    let time = ends_at
        .to_zoned_datetime(TimeZone::system())
        .strftime("%H:%M");
    println!(
        "{}",
        localizer.text(
            "cli-test-lock-started",
            &[
                ("label", Arg::Text(label)),
                ("time", Arg::Text(time.to_string())),
            ],
        )
    );
    Ok(ExitCode::SUCCESS)
}
