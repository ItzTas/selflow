use std::process::ExitCode;

use clap::Args;
use jiff::tz::TimeZone;
use selflow_client::client::Client;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_platform::paths::socket_path;
use selflow_protocol::ipc::{ExtensionSource, Request, Response};

use super::config_cmd::unexpected;
use crate::cli::CliError;
use crate::prompt::{
    Challenge, Prompt, TerminalPrompt, answer_challenges, clock_time, refusal_text,
};

#[derive(Debug, Args)]
pub struct UnlockArgs {}

pub fn unlock_cmd(_args: UnlockArgs) -> Result<ExitCode, CliError> {
    let path = socket_path().ok_or(CliError::NoRuntimeDir)?;
    let mut client = Client::connect(&path)?;
    unlock_with(&mut client, &mut TerminalPrompt, &TimeZone::system())
}

pub fn unlock_with(
    client: &mut Client,
    prompt: &mut dyn Prompt,
    tz: &TimeZone,
) -> Result<ExitCode, CliError> {
    let status = match client.request(Request::Status)? {
        Response::Status(status) => status,
        other => return Err(unexpected(other)),
    };
    let language = Language::from_tag(&status.language).unwrap_or_else(Language::from_env);
    let localizer = Localizer::new(language);
    let Some(lock) = status.locked else {
        return release_screen(client, &localizer, prompt);
    };
    let answer = answer_challenges(
        &Challenge::from(&lock),
        &localizer,
        tz,
        prompt,
        &mut |reason, phrase| {
            let request = Request::RequestExtension {
                reason,
                phrase,
                source: ExtensionSource::Terminal,
            };
            Ok(client.request(request)?)
        },
    )?;
    match answer {
        Response::ExtensionGranted { until } => {
            let time = Arg::Text(clock_time(until, tz));
            prompt.say(&localizer.text("cli-unlock-granted", &[("time", time)]));
            Ok(ExitCode::SUCCESS)
        }
        Response::ExtensionRefused(refusal) => {
            prompt.say(&refusal_text(&refusal, &localizer, tz));
            Ok(ExitCode::from(1))
        }
        other => Err(unexpected(other)),
    }
}

fn release_screen(
    client: &mut Client,
    localizer: &Localizer,
    prompt: &mut dyn Prompt,
) -> Result<ExitCode, CliError> {
    match client.request(Request::ReleaseScreen)? {
        Response::Released => {
            prompt.say(&localizer.text("cli-unlock-not-locked", &[]));
            Ok(ExitCode::SUCCESS)
        }
        other => Err(unexpected(other)),
    }
}
