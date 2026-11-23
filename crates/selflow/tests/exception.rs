mod support;

use std::process::ExitCode;

use selflow::cli::commands::exception_with;
use selflow_protocol::ipc::{ChallengeView, RefusalView, Request, Response, StatusView};
use support::{FakeDaemon, ScriptedPrompt, at, brt};

#[test]
fn exception_waits_asks_and_reports() {
    let daemon = FakeDaemon::new([
        Response::Status(StatusView {
            language: "en".to_owned(),
            locked: None,
            next_lock_at: None,
        }),
        Response::ExceptionChallenge(ChallengeView {
            label: "bedtime".to_owned(),
            available_at: at(19, 20, 2).timestamp(),
            reason_required: true,
            phrase: Some("I choose to rest".to_owned()),
            extensions_left: 1,
        }),
        Response::ExceptionRefused(RefusalView::PhraseMismatch),
        Response::ExceptionGranted {
            label: "bedtime".to_owned(),
        },
    ]);
    let mut prompt = ScriptedPrompt::new([
        "on call tonight",
        "I choose to stay",
        "on call tonight",
        "I choose to rest",
    ]);
    let code = exception_with(&mut daemon.client(), "bedtime", &mut prompt, &brt()).unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
    let confirm = |phrase: &str| Request::ConfirmException {
        rule: "bedtime".to_owned(),
        reason: "on call tonight".to_owned(),
        phrase: phrase.to_owned(),
    };
    assert_eq!(
        daemon.requests(),
        [
            Request::Status,
            Request::StartException {
                rule: "bedtime".to_owned()
            },
            confirm("I choose to stay"),
            confirm("I choose to rest"),
        ]
    );
    assert_eq!(prompt.waits(), [at(19, 20, 2).timestamp()]);
    assert_eq!(
        prompt.said(),
        [
            "You can skip the bedtime rule from 20:02.",
            "The sentence doesn't match yet. Check it and try again.",
            "The bedtime rule is off for the rest of the day.",
        ]
    );
}
