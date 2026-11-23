mod support;

use jiff::Timestamp;
use jiff::civil::date;
use selflow::render::{render_config_status, render_errors, render_report, render_saved};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{
    ChangeView, ConfigStatusView, MessageView, PositionedMessage, ReportEntry, ReportView,
};
use support::{at, brt};

fn change(id: &str, old: &str, new: &str, effective_at: Timestamp) -> ChangeView {
    let args = [
        ("rule", "bedtime"),
        ("days", "thu"),
        ("old", old),
        ("new", new),
    ];
    ChangeView {
        message: MessageView {
            id: id.to_owned(),
            args: args
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
        effective_at,
    }
}

#[test]
fn renders_errors_with_line_and_column_in_portuguese() {
    let error = PositionedMessage {
        message: MessageView {
            id: "config-bad-duration".into(),
            args: vec![("value".into(), "soon".into())],
        },
        line: 15,
        column: 8,
    };
    let out = render_errors(&Localizer::new(Language::PtBr), &[error]);
    assert!(out.starts_with("linha 15, coluna 8: "));
    assert!(out.contains("soon"));
}

#[test]
fn a_save_lists_what_applies_now_and_what_waits() {
    let saved_at = at(18, 12, 0).timestamp();
    let changes = [
        change("change-rule-starts-earlier", "23:00", "22:00", saved_at),
        change(
            "change-more-extensions",
            "1",
            "3",
            at(19, 12, 0).timestamp(),
        ),
    ];
    let en = render_saved(&Localizer::new(Language::En), saved_at, &changes, &brt());
    assert_eq!(
        en,
        "Saved.\n\
         Applies now:\n\
         - The rule `bedtime` starts earlier on thu: 23:00 → 22:00.\n\
         Waits until Thu 19 Nov, 12:00:\n\
         - For the rule `bedtime`, each day allows more extensions on thu: 1 → 3."
    );
    let pt = render_saved(&Localizer::new(Language::PtBr), saved_at, &changes, &brt());
    assert!(pt.starts_with("Salvo.\n"));
    assert!(pt.contains("qui 19 nov, 12:00"));
}

#[test]
fn a_save_that_only_waits_confirms_it_then_lists_the_wait() {
    let changes = [change(
        "change-rule-starts-later",
        "23:00",
        "01:00",
        at(19, 12, 0).timestamp(),
    )];
    let en = render_saved(
        &Localizer::new(Language::En),
        at(18, 12, 0).timestamp(),
        &changes,
        &brt(),
    );
    assert_eq!(
        en,
        "Saved.\n\
         Waits until Thu 19 Nov, 12:00:\n\
         - The rule `bedtime` starts later on thu: 23:00 → 01:00."
    );
}

#[test]
fn a_save_without_changes_says_so() {
    let en = render_saved(
        &Localizer::new(Language::En),
        at(18, 12, 0).timestamp(),
        &[],
        &brt(),
    );
    assert_eq!(en, "Saved.\nNothing changes in how your rules work.");
}

#[test]
fn the_status_before_the_first_save_says_no_rule_runs_yet() {
    let status = ConfigStatusView {
        last_saved_at: None,
        pending: Vec::new(),
    };
    let out = render_config_status(&Localizer::new(Language::En), &status, &brt());
    assert_eq!(
        out,
        "No rule runs until you save your settings for the first time. To save them, run selflow config edit.\n\
         No changes are waiting."
    );
}

#[test]
fn renders_events_at_their_time_and_the_waiting_changes_on_their_dates() {
    let report = ReportView {
        logical_date: date(2026, 11, 19),
        entries: vec![ReportEntry {
            at: at(19, 23, 0).timestamp(),
            message: MessageView {
                id: "report-lock-started".into(),
                args: vec![("label".into(), "bedtime".into())],
            },
        }],
        pending: vec![change(
            "change-more-extensions",
            "1",
            "3",
            at(20, 10, 0).timestamp(),
        )],
    };
    let out = render_report(&Localizer::new(Language::En), &report, &brt());
    assert_eq!(
        out,
        "Report for 2026-11-19\n\
         23:00  The bedtime lock started.\n\
         Changes still waiting:\n\
         Fri 20 Nov, 10:00  For the rule `bedtime`, each day allows more extensions on thu: 1 → 3."
    );
}
