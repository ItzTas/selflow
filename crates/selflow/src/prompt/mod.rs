use std::io::{self, Write};
use std::time::Duration;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::{ChallengeView, LockView, RefusalView, Response};

use crate::cli::CliError;

pub trait Prompt {
    fn ask(&mut self, question: &str) -> Option<String>;
    fn say(&mut self, text: &str);
    fn wait_until(&mut self, at: Timestamp);
}

pub struct TerminalPrompt;

impl Prompt for TerminalPrompt {
    fn ask(&mut self, question: &str) -> Option<String> {
        print!("{question}\n> ");
        let _ = io::stdout().flush();
        let mut answer = String::new();
        if matches!(io::stdin().read_line(&mut answer), Ok(0) | Err(_)) {
            println!();
            return None;
        }
        Some(answer.trim_end_matches(['\n', '\r']).to_owned())
    }

    fn say(&mut self, text: &str) {
        println!("{text}");
    }

    fn wait_until(&mut self, at: Timestamp) {
        if let Ok(left) = Duration::try_from(Timestamp::now().duration_until(at)) {
            std::thread::sleep(left);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub label: String,
    pub available_at: Timestamp,
    pub reason_required: bool,
    pub phrase: Option<String>,
    pub extensions_left: u32,
    pub kind: ChallengeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeKind {
    Unlock,
    Exception,
}

impl ChallengeKind {
    fn wait_id(self) -> &'static str {
        match self {
            ChallengeKind::Unlock => "cli-unlock-wait",
            ChallengeKind::Exception => "cli-exception-wait",
        }
    }

    fn reason_id(self) -> &'static str {
        match self {
            ChallengeKind::Unlock => "lock-reason-label",
            ChallengeKind::Exception => "cli-exception-reason",
        }
    }

    fn refused(self, refusal: RefusalView) -> Response {
        match self {
            ChallengeKind::Unlock => Response::ExtensionRefused(refusal),
            ChallengeKind::Exception => Response::ExceptionRefused(refusal),
        }
    }
}

impl From<&LockView> for Challenge {
    fn from(view: &LockView) -> Challenge {
        Challenge {
            label: view.label.clone(),
            available_at: view.extension_available_at,
            reason_required: view.reason_required,
            phrase: view.phrase.clone(),
            extensions_left: view.extensions_left,
            kind: ChallengeKind::Unlock,
        }
    }
}

impl From<&ChallengeView> for Challenge {
    fn from(view: &ChallengeView) -> Challenge {
        Challenge {
            label: view.label.clone(),
            available_at: view.available_at,
            reason_required: view.reason_required,
            phrase: view.phrase.clone(),
            extensions_left: view.extensions_left,
            kind: ChallengeKind::Exception,
        }
    }
}

pub fn answer_challenges(
    challenge: &Challenge,
    localizer: &Localizer,
    tz: &TimeZone,
    prompt: &mut dyn Prompt,
    submit: &mut dyn FnMut(String, String) -> Result<Response, CliError>,
) -> Result<Response, CliError> {
    if challenge.extensions_left == 0 {
        return Ok(challenge.kind.refused(RefusalView::NoBudget));
    }
    let mut wait = Some(challenge.available_at);
    let mut last = None;
    loop {
        if let Some(at) = wait.take() {
            let args = [
                ("label", Arg::Text(challenge.label.clone())),
                ("time", Arg::Text(clock_time(at, tz))),
            ];
            prompt.say(&localizer.text(challenge.kind.wait_id(), &args));
            prompt.wait_until(at);
        }
        let Some((reason, phrase)) = answers(challenge, localizer, prompt) else {
            return last.ok_or_else(input_ended);
        };
        let response = submit(reason, phrase)?;
        match refusal_of(&response) {
            Some(RefusalView::TooEarly { available_at }) => wait = Some(*available_at),
            Some(refusal @ (RefusalView::ReasonTooShort { .. } | RefusalView::PhraseMismatch)) => {
                prompt.say(&refusal_text(refusal, localizer, tz));
            }
            _ => return Ok(response),
        }
        last = Some(response);
    }
}

pub fn refusal_text(refusal: &RefusalView, localizer: &Localizer, tz: &TimeZone) -> String {
    match refusal {
        RefusalView::TooEarly { available_at } => localizer.text(
            "lock-refused-too-early",
            &[("time", Arg::Text(clock_time(*available_at, tz)))],
        ),
        RefusalView::NoBudget => localizer.text("lock-refused-no-budget", &[]),
        RefusalView::ReasonTooShort { min_chars } => localizer.text(
            "lock-refused-reason-too-short",
            &[("min_chars", Arg::Number(*min_chars as i64))],
        ),
        RefusalView::PhraseMismatch => localizer.text("lock-refused-phrase-mismatch", &[]),
    }
}

pub fn clock_time(at: Timestamp, tz: &TimeZone) -> String {
    at.to_zoned_datetime(tz.clone())
        .strftime("%H:%M")
        .to_string()
}

fn answers(
    challenge: &Challenge,
    localizer: &Localizer,
    prompt: &mut dyn Prompt,
) -> Option<(String, String)> {
    let reason = match challenge.reason_required {
        true => prompt.ask(&localizer.text(challenge.kind.reason_id(), &[]))?,
        false => String::new(),
    };
    let phrase = match &challenge.phrase {
        Some(phrase) => prompt.ask(&localizer.text(
            "lock-phrase-label",
            &[("phrase", Arg::Text(phrase.clone()))],
        ))?,
        None => String::new(),
    };
    Some((reason, phrase))
}

fn refusal_of(response: &Response) -> Option<&RefusalView> {
    match response {
        Response::ExtensionRefused(refusal) | Response::ExceptionRefused(refusal) => Some(refusal),
        _ => None,
    }
}

fn input_ended() -> CliError {
    CliError::Protocol(io::Error::from(io::ErrorKind::UnexpectedEof))
}
