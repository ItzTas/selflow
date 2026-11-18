mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use jiff::civil::date;
use selflow_core::config::parse;
use selflow_core::enforce::NightLog;
use selflow_core::plan::windows_for;
use selflow_core::timing::Seed;
use selflow_core::versions::ChangeKind;
use selflowd::store::{Event, EventKind, Store, default_path_from};
use support::{at, brt, extension, no_jitter};

#[test]
fn keeps_one_seed_for_the_life_of_the_database() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.seed().unwrap(), store.seed().unwrap());
}

#[test]
fn a_new_save_replaces_the_pending_version() {
    let store = Store::open_in_memory().unwrap();
    let ts = |day, hour| at(day, hour, 0).timestamp();
    let first = store
        .save_version("one", ts(18, 10), ts(18, 10), ChangeKind::Tighten)
        .unwrap();
    store
        .save_version("two", ts(18, 11), ts(19, 11), ChangeKind::Loosen)
        .unwrap();
    let third = store
        .save_version("three", ts(18, 12), ts(18, 12), ChangeKind::Tighten)
        .unwrap();
    assert_eq!(
        store
            .versions()
            .unwrap()
            .iter()
            .map(|v| v.id)
            .collect::<Vec<_>>(),
        vec![first, third]
    );
    assert_eq!(store.latest_text().unwrap().as_deref(), Some("three"));
}

#[test]
fn records_an_event_only_once() {
    let store = Store::open_in_memory().unwrap();
    let event = Event {
        at: at(19, 23, 0).timestamp(),
        logical_date: date(2026, 11, 19),
        kind: EventKind::LockStarted,
        key: "weeknight/bedtime/2026-11-19/2026-11-20T02:00:00Z".into(),
        detail: Some(r#"{"label":"bedtime"}"#.into()),
    };
    assert!(store.record_event(&event).unwrap());
    assert!(!store.record_event(&event).unwrap());
    assert_eq!(store.events_for(date(2026, 11, 19)).unwrap().len(), 1);
}

#[test]
fn reads_back_the_log_of_the_requested_nights() {
    let store = Store::open_in_memory().unwrap();
    let extension = extension((23, 5), (23, 25));
    let window = windows_for(
        &parse(&no_jitter()).unwrap(),
        Seed(7),
        date(2026, 11, 19),
        &brt(),
    )
    .remove(0);
    store.record_extension(&extension).unwrap();
    store.record_started(&window).unwrap();
    store.record_started(&window).unwrap();
    let log = store.night_log(&[date(2026, 11, 19)]).unwrap();
    assert_eq!(
        (log.extensions, log.started),
        (vec![extension], vec![window])
    );
    assert_eq!(
        store.night_log(&[date(2026, 11, 18)]).unwrap(),
        NightLog::default()
    );
}

#[test]
fn creates_files_only_the_user_can_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/selflow.db");
    Store::open(&path).unwrap();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&path), mode(path.parent().unwrap())), (0o600, 0o700));
}

#[test]
fn the_default_path_follows_xdg_state_home() {
    assert_eq!(
        default_path_from(Some("/s".as_ref()), Some("/h".as_ref())),
        Some("/s/selflow/selflow.db".into())
    );
    assert_eq!(
        default_path_from(None, Some("/h".as_ref())),
        Some("/h/.local/state/selflow/selflow.db".into())
    );
}
