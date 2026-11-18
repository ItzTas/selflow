mod support;

use selflow::lock::{ExtensionArea, LockEffect, LockEvent, LockModel, Screen};
use selflow_protocol::ipc::{LockView, RefusalView};
use support::{at, view};

#[test]
fn shows_the_wait_and_then_the_form() {
    let mut model = LockModel::new(at(19, 23, 0).timestamp());
    model.update(LockEvent::View(view()));
    assert!(
        matches!(model.screen(), Screen::Locked { extension: ExtensionArea::WaitUntil(t), .. } if t == at(19, 23, 2).timestamp())
    );
    model.update(LockEvent::Tick(at(19, 23, 2).timestamp()));
    assert!(matches!(
        model.screen(),
        Screen::Locked {
            extension: ExtensionArea::Form {
                minutes: 20,
                reason_required: true,
                ..
            },
            ..
        }
    ));
}

#[test]
fn sends_one_request_until_an_answer_arrives() {
    let mut model = LockModel::new(at(19, 23, 2).timestamp());
    model.update(LockEvent::View(view()));
    model.update(LockEvent::ReasonEdited("talking to my sister".into()));
    assert_eq!(
        model.update(LockEvent::Submit),
        LockEffect::SendExtensionRequest {
            reason: "talking to my sister".into(),
            phrase: String::new()
        }
    );
    assert_eq!(model.update(LockEvent::Submit), LockEffect::None);
    model.update(LockEvent::Refused(RefusalView::ReasonTooShort {
        min_chars: 10,
    }));
    assert!(matches!(
        model.screen(),
        Screen::Locked {
            extension: ExtensionArea::Form {
                error: Some(_),
                sending: false,
                ..
            },
            ..
        }
    ));
}

#[test]
fn the_end_of_the_lock_exits() {
    let mut model = LockModel::new(at(19, 23, 0).timestamp());
    model.update(LockEvent::View(view()));
    assert_eq!(model.update(LockEvent::Ended), LockEffect::Exit);
}

#[test]
fn a_lost_connection_keeps_the_screen_locked() {
    let mut model = LockModel::new(at(19, 23, 0).timestamp());
    model.update(LockEvent::View(view()));
    assert_eq!(model.update(LockEvent::Disconnected), LockEffect::Reconnect);
    assert!(matches!(model.screen(), Screen::Locked { .. }));
}

#[test]
fn shows_no_form_when_the_budget_is_spent() {
    let mut model = LockModel::new(at(19, 23, 30).timestamp());
    model.update(LockEvent::View(LockView {
        extensions_left: 0,
        ..view()
    }));
    assert!(matches!(
        model.screen(),
        Screen::Locked {
            extension: ExtensionArea::NoneLeft,
            ..
        }
    ));
}
