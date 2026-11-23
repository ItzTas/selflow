mod support;

use std::process::ExitCode;

use jiff::Timestamp;
use selflow::cli::commands::config_edit_with;
use selflow::editor::{EditOutcome, FirstSave, edit_loop};
use selflow_protocol::ipc::{ConfigStatusView, Request, Response, StatusView};
use support::{FakeDaemon, ScriptedEditor, at, brt, invalid, saved};

#[test]
fn reopens_the_editor_on_the_rejected_text() {
    let mut editor = ScriptedEditor::new(["bad", "good"]);
    let mut sent = Vec::new();
    let outcome = edit_loop(
        "initial",
        FirstSave::Done,
        &mut editor,
        &mut |text| {
            sent.push(text.to_owned());
            if text == "bad" { invalid() } else { saved() }
        },
        &mut |_| true,
    );
    assert_eq!(editor.seen(), vec!["initial", "bad"]);
    assert_eq!(sent, vec!["bad", "good"]);
    assert!(matches!(outcome, EditOutcome::Saved(_)));
}

#[test]
fn stops_when_the_person_declines_to_edit_again() {
    let mut editor = ScriptedEditor::new(["bad"]);
    let outcome = edit_loop(
        "initial",
        FirstSave::Done,
        &mut editor,
        &mut |_| invalid(),
        &mut |_| false,
    );
    assert!(matches!(outcome, EditOutcome::Abandoned));
}

#[test]
fn a_pending_first_save_sends_the_unchanged_file_once() {
    let mut editor = ScriptedEditor::new(["initial", "initial"]);
    let mut sent = Vec::new();
    let outcome = edit_loop(
        "initial",
        FirstSave::Pending,
        &mut editor,
        &mut |text| {
            sent.push(text.to_owned());
            invalid()
        },
        &mut |_| true,
    );
    assert_eq!(sent, vec!["initial"]);
    assert!(matches!(outcome, EditOutcome::Abandoned));
}

fn english() -> Response {
    Response::Status(StatusView {
        language: "en".to_owned(),
        locked: None,
        next_lock_at: None,
    })
}

fn config_status(last_saved_at: Option<Timestamp>) -> Response {
    Response::ConfigStatus(ConfigStatusView {
        last_saved_at,
        pending: Vec::new(),
    })
}

#[test]
fn with_nothing_saved_yet_an_unchanged_file_is_saved() {
    let daemon = FakeDaemon::new([
        english(),
        config_status(None),
        Response::Config {
            text: "default".to_owned(),
        },
        saved(),
    ]);
    let mut editor = ScriptedEditor::new(["default"]);
    let code = config_edit_with(&mut daemon.client(), &mut editor, &brt()).unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        daemon.requests(),
        [
            Request::Status,
            Request::ConfigStatus,
            Request::GetConfig,
            Request::SaveConfig {
                text: "default".to_owned()
            },
        ]
    );
}

#[test]
fn once_something_is_saved_an_unchanged_file_is_not_sent() {
    let daemon = FakeDaemon::new([
        english(),
        config_status(Some(at(18, 10, 0).timestamp())),
        Response::Config {
            text: "saved".to_owned(),
        },
    ]);
    let mut editor = ScriptedEditor::new(["saved"]);
    let code = config_edit_with(&mut daemon.client(), &mut editor, &brt()).unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        daemon.requests(),
        [Request::Status, Request::ConfigStatus, Request::GetConfig]
    );
}
