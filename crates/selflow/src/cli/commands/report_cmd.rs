use std::process::ExitCode;

use jiff::tz::TimeZone;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{Request, Response, socket_path};

use super::config_cmd::{daemon_language, unexpected};
use crate::cli::{CliError, ReportArgs};
use crate::client::Client;
use crate::render::render_report;

pub fn report_cmd(args: ReportArgs) -> Result<ExitCode, CliError> {
    let path = socket_path().ok_or(CliError::NoRuntimeDir)?;
    let mut client = Client::connect(&path)?;
    let localizer = Localizer::new(daemon_language(&mut client)?);
    let request = Request::Report {
        logical_date: args.date,
    };
    match client.request(request)? {
        Response::Report(view) => {
            println!("{}", render_report(&localizer, &view, &TimeZone::system()));
            Ok(ExitCode::SUCCESS)
        }
        other => Err(unexpected(other)),
    }
}
