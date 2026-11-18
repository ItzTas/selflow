mod support;

use selflow::editor::{EditOutcome, edit_loop};
use support::{ScriptedEditor, invalid, saved};

#[test]
fn reopens_the_editor_on_the_rejected_text() {
    let mut editor = ScriptedEditor::new(["bad", "good"]);
    let mut sent = Vec::new();
    let outcome = edit_loop(
        "initial",
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
    let outcome = edit_loop("initial", &mut editor, &mut |_| invalid(), &mut |_| false);
    assert!(matches!(outcome, EditOutcome::Abandoned));
}

#[test]
fn does_not_send_an_unchanged_file() {
    let mut editor = ScriptedEditor::new(["initial"]);
    let outcome = edit_loop(
        "initial",
        &mut editor,
        &mut |_| panic!("nothing should be sent"),
        &mut |_| true,
    );
    assert!(matches!(outcome, EditOutcome::Unchanged));
}
