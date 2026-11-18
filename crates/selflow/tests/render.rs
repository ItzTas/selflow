mod support;

use jiff::civil::date;
use selflow::render::{render_errors, render_report, render_saved};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use selflow_protocol::ipc::{
    ChangeKindView, MessageView, PositionedMessage, ReportEntry, ReportView,
};
use support::{at, brt};

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
fn renders_when_a_loosening_takes_effect() {
    let effective = at(20, 10, 0).timestamp();
    let pt = render_saved(
        &Localizer::new(Language::PtBr),
        ChangeKindView::Loosen,
        effective,
        &[],
        &brt(),
    );
    let en = render_saved(
        &Localizer::new(Language::En),
        ChangeKindView::Loosen,
        effective,
        &[],
        &brt(),
    );
    assert!(pt.contains("sex 20 nov, 10:00"));
    assert!(en.contains("Fri 20 Nov, 10:00"));
}

#[test]
fn renders_events_at_their_time_and_a_waiting_change_on_its_date() {
    let effective = at(20, 10, 0).timestamp();
    let report = ReportView {
        logical_date: date(2026, 11, 19),
        entries: vec![
            ReportEntry {
                at: at(19, 23, 0).timestamp(),
                message: MessageView {
                    id: "report-lock-started".into(),
                    args: vec![("label".into(), "bedtime".into())],
                },
            },
            ReportEntry {
                at: effective,
                message: MessageView {
                    id: "report-pending".into(),
                    args: vec![("when".into(), effective.to_string())],
                },
            },
        ],
    };
    let out = render_report(&Localizer::new(Language::En), &report, &brt());
    assert_eq!(
        out,
        "Report for 2026-11-19\n\
         23:00  The bedtime lock started.\n\
         A saved change takes effect on Fri 20 Nov, 10:00."
    );
}
