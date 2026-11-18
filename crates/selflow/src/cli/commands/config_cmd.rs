use std::process::ExitCode;

use jiff::tz::TimeZone;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{MessageView, Request, Response, socket_path};

use crate::cli::{CliError, ConfigAction, ConfigArgs};
use crate::client::Client;
use crate::editor::{EditOutcome, TempfileEditor, edit_loop};
use crate::render::{message_text, render_config_status, render_errors, render_saved};

pub fn config_cmd(args: ConfigArgs) -> Result<ExitCode, CliError> {
    let path = socket_path().ok_or(CliError::NoRuntimeDir)?;
    let mut client = Client::connect(&path)?;
    let localizer = Localizer::new(daemon_language(&mut client)?);
    let tz = TimeZone::system();
    match args.action {
        ConfigAction::Status => status(&mut client, &localizer, &tz),
        ConfigAction::Edit => edit(&mut client, &localizer, &tz),
    }
}

pub(super) fn daemon_language(client: &mut Client) -> Result<Language, CliError> {
    match client.request(Request::Status)? {
        Response::Status(status) => {
            Ok(Language::from_tag(&status.language).unwrap_or_else(Language::from_env))
        }
        other => Err(unexpected(other)),
    }
}

pub(super) fn unexpected(response: Response) -> CliError {
    match response {
        Response::Error(message) => CliError::Daemon(message),
        _ => CliError::UnexpectedResponse,
    }
}

fn status(client: &mut Client, localizer: &Localizer, tz: &TimeZone) -> Result<ExitCode, CliError> {
    match client.request(Request::ConfigStatus)? {
        Response::ConfigStatus(view) => {
            println!("{}", render_config_status(localizer, &view, tz));
            Ok(ExitCode::SUCCESS)
        }
        other => Err(unexpected(other)),
    }
}

fn edit(client: &mut Client, localizer: &Localizer, tz: &TimeZone) -> Result<ExitCode, CliError> {
    let initial = match client.request(Request::GetConfig)? {
        Response::Config { text } => text,
        other => return Err(unexpected(other)),
    };
    let outcome = edit_loop(
        &initial,
        &mut TempfileEditor::new(),
        &mut |text| {
            client
                .request(Request::SaveConfig {
                    text: text.to_owned(),
                })
                .unwrap_or_else(|_| {
                    Response::Error(MessageView {
                        id: "cli-protocol-error".into(),
                        args: Vec::new(),
                    })
                })
        },
        &mut |errors| {
            println!("{}", render_errors(localizer, errors));
            ask_to_edit_again(localizer)
        },
    );
    match outcome {
        EditOutcome::Unchanged => {
            println!("{}", localizer.text("cli-config-unchanged", &[]));
            Ok(ExitCode::SUCCESS)
        }
        EditOutcome::Abandoned => {
            println!("{}", localizer.text("cli-config-abandoned", &[]));
            Ok(ExitCode::from(1))
        }
        EditOutcome::Failed(error) => Err(CliError::Editor(error)),
        EditOutcome::Saved(Response::Saved {
            kind,
            effective_at,
            changes,
        }) => {
            println!(
                "{}",
                render_saved(localizer, kind, effective_at, &changes, tz)
            );
            Ok(ExitCode::SUCCESS)
        }
        EditOutcome::Saved(Response::Error(message)) => {
            eprintln!("{}", message_text(localizer, &message));
            Ok(ExitCode::from(1))
        }
        EditOutcome::Saved(other) => Err(unexpected(other)),
    }
}

fn ask_to_edit_again(localizer: &Localizer) -> bool {
    print!("{} ", localizer.text("cli-edit-again", &[]));
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    let answer = answer.trim().to_lowercase();
    answer.is_empty() || ["y", "yes", "s", "sim"].contains(&answer.as_str())
}
