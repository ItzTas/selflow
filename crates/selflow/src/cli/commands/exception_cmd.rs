use std::process::ExitCode;

use clap::Args;
use jiff::tz::TimeZone;
use selflow_client::client::Client;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_platform::paths::socket_path;
use selflow_protocol::ipc::{RefusalView, Request, Response};

use super::config_cmd::{daemon_language, unexpected};
use crate::cli::CliError;
use crate::prompt::{Challenge, Prompt, TerminalPrompt, answer_challenges, refusal_text};

#[derive(Debug, Args)]
pub struct ExceptionArgs {
    #[arg(long)]
    pub rule: String,
}

pub fn exception_cmd(args: ExceptionArgs) -> Result<ExitCode, CliError> {
    let path = socket_path().ok_or(CliError::NoRuntimeDir)?;
    let mut client = Client::connect(&path)?;
    exception_with(
        &mut client,
        &args.rule,
        &mut TerminalPrompt,
        &TimeZone::system(),
    )
}

pub fn exception_with(
    client: &mut Client,
    rule: &str,
    prompt: &mut dyn Prompt,
    tz: &TimeZone,
) -> Result<ExitCode, CliError> {
    let localizer = Localizer::new(daemon_language(client)?);
    let start = Request::StartException {
        rule: rule.to_owned(),
    };
    let view = match client.request(start)? {
        Response::ExceptionChallenge(view) => view,
        other => return Err(unexpected(other)),
    };
    let answer = answer_challenges(
        &Challenge::from(&view),
        &localizer,
        tz,
        prompt,
        &mut |reason, phrase| {
            let request = Request::ConfirmException {
                rule: rule.to_owned(),
                reason,
                phrase,
            };
            Ok(client.request(request)?)
        },
    )?;
    match answer {
        Response::ExceptionGranted { label } => {
            let label = ("label", Arg::Text(label));
            prompt.say(&localizer.text("cli-exception-granted", &[label]));
            Ok(ExitCode::SUCCESS)
        }
        Response::ExceptionRefused(RefusalView::NoBudget) => {
            let label = ("label", Arg::Text(view.label));
            prompt.say(&localizer.text("cli-exception-no-budget", &[label]));
            Ok(ExitCode::from(1))
        }
        Response::ExceptionRefused(refusal) => {
            prompt.say(&refusal_text(&refusal, &localizer, tz));
            Ok(ExitCode::from(1))
        }
        other => Err(unexpected(other)),
    }
}
