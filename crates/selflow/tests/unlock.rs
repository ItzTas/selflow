mod support;

use std::process::ExitCode;

use selflow::cli::commands::unlock_with;
use selflow::prompt::{Challenge, answer_challenges};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{
    ExtensionSource, LockView, RefusalView, Request, Response, StatusView,
};
use support::{FakeDaemon, ScriptedPrompt, at, brt};

fn challenge(phrase: Option<&str>, extensions_left: u32) -> Challenge {
    Challenge {
        label: "bedtime".to_owned(),
        available_at: at(19, 23, 2).timestamp(),
        reason_required: true,
        phrase: phrase.map(str::to_owned),
        extensions_left,
    }
}

fn granted() -> Response {
    Response::ExtensionGranted {
        until: at(19, 23, 22).timestamp(),
    }
}

fn english() -> Localizer {
    Localizer::new(Language::En)
}

#[test]
fn waits_then_asks_the_reason_and_the_phrase() {
    let mut prompt = ScriptedPrompt::new(["talking to my sister", "I choose to stay up"]);
    let mut sent = Vec::new();
    let answer = answer_challenges(
        &challenge(Some("I choose to stay up"), 1),
        &english(),
        &brt(),
        &mut prompt,
        &mut |reason, phrase| {
            sent.push((reason, phrase));
            Ok(granted())
        },
    )
    .unwrap();
    assert_eq!(answer, granted());
    assert_eq!(prompt.waits(), [at(19, 23, 2).timestamp()]);
    assert_eq!(prompt.said(), ["You can ask for more time from 23:02."]);
    assert_eq!(
        sent,
        [(
            "talking to my sister".to_owned(),
            "I choose to stay up".to_owned()
        )]
    );
}

#[test]
fn asks_again_after_a_short_reason() {
    let mut prompt = ScriptedPrompt::new(["tired", "talking to my sister"]);
    let mut sent = Vec::new();
    let answer = answer_challenges(
        &challenge(None, 1),
        &english(),
        &brt(),
        &mut prompt,
        &mut |reason, phrase| {
            let short = reason.chars().count() < 10;
            sent.push((reason, phrase));
            if short {
                return Ok(Response::ExtensionRefused(RefusalView::ReasonTooShort {
                    min_chars: 10,
                }));
            }
            Ok(granted())
        },
    )
    .unwrap();
    assert_eq!(answer, granted());
    assert_eq!(
        sent,
        [
            ("tired".to_owned(), String::new()),
            ("talking to my sister".to_owned(), String::new()),
        ]
    );
    assert_eq!(
        prompt.said()[1..],
        ["Write a little more: at least 10 characters."]
    );
}

#[test]
fn stops_when_no_extension_is_left() {
    let mut prompt = ScriptedPrompt::new([]);
    let answer = answer_challenges(
        &challenge(None, 0),
        &english(),
        &brt(),
        &mut prompt,
        &mut |_, _| panic!("nothing should be submitted"),
    )
    .unwrap();
    assert_eq!(answer, Response::ExtensionRefused(RefusalView::NoBudget));
    assert_eq!(prompt.waits(), []);
}

#[test]
fn unlock_with_nothing_locked_releases_the_screen() {
    let daemon = FakeDaemon::new([
        Response::Status(StatusView {
            language: "en".to_owned(),
            locked: None,
            next_lock_at: None,
        }),
        Response::Released,
    ]);
    let mut prompt = ScriptedPrompt::new([]);
    let code = unlock_with(&mut daemon.client(), &mut prompt, &brt()).unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(daemon.requests(), [Request::Status, Request::ReleaseScreen]);
    assert_eq!(
        prompt.said(),
        [english().text("cli-unlock-not-locked", &[])]
    );
}

#[test]
fn unlock_with_a_lock_asks_for_more_time_from_the_terminal() {
    let lock = LockView {
        label: "bedtime".to_owned(),
        ends_at: at(20, 7, 0).timestamp(),
        extension_available_at: at(19, 23, 2).timestamp(),
        extensions_left: 1,
        extension_minutes: 20,
        reason_required: true,
        phrase: None,
        shutdown_at: None,
        language: "en".to_owned(),
    };
    let daemon = FakeDaemon::new([
        Response::Status(StatusView {
            language: "en".to_owned(),
            locked: Some(lock),
            next_lock_at: None,
        }),
        granted(),
    ]);
    let mut prompt = ScriptedPrompt::new(["talking to my sister"]);
    let code = unlock_with(&mut daemon.client(), &mut prompt, &brt()).unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        daemon.requests(),
        [
            Request::Status,
            Request::RequestExtension {
                reason: "talking to my sister".to_owned(),
                phrase: String::new(),
                source: ExtensionSource::Terminal,
            },
        ]
    );
    assert_eq!(prompt.said().last().unwrap(), "Unlocked until 23:22.");
}
