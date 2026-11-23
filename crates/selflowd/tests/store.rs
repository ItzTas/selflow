mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use jiff::civil::date;
use selflow_core::config::parse;
use selflow_core::enforce::DayLog;
use selflow_core::plan::{RuleWindow, windows_for};
use selflow_core::timing::Seed;
use selflow_core::versions::{ChangeKind, StoredChange, StoredVersion};
use selflowd::store::{Event, EventKind, Store};
use support::{M1_TEXT, at, brt, extension, m1_store, no_jitter};

#[test]
fn keeps_one_seed_for_the_life_of_the_database() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.seed().unwrap(), store.seed().unwrap());
}

#[test]
fn a_save_keeps_each_change_with_its_time() {
    let store = Store::open_in_memory().unwrap();
    let ts = |day, hour| at(day, hour, 0).timestamp();
    let later = ts(19, 12) + jiff::SignedDuration::from_millis(500);
    let change = |unit: &str, kind, effective_at| StoredChange {
        version_id: 0,
        unit: unit.to_owned(),
        kind,
        effective_at,
    };
    let first = store
        .save_version(
            "one",
            ts(18, 10),
            &[change("language", ChangeKind::Neutral, ts(18, 10))],
        )
        .unwrap();
    let second = store
        .save_version(
            "two",
            ts(18, 12),
            &[
                change("thu/bedtime/timing", ChangeKind::Tighten, ts(18, 12)),
                change("thu/bedtime/extensions", ChangeKind::Loosen, later),
            ],
        )
        .unwrap();
    let (versions, changes) = store.history_rows().unwrap();
    assert_eq!(
        versions,
        vec![
            StoredVersion {
                id: first,
                saved_at: ts(18, 10),
                text: "one".to_owned(),
            },
            StoredVersion {
                id: second,
                saved_at: ts(18, 12),
                text: "two".to_owned(),
            },
        ]
    );
    assert_eq!(
        changes,
        vec![
            StoredChange {
                version_id: first,
                ..change("language", ChangeKind::Neutral, ts(18, 10))
            },
            StoredChange {
                version_id: second,
                ..change("thu/bedtime/timing", ChangeKind::Tighten, ts(18, 12))
            },
            StoredChange {
                version_id: second,
                ..change("thu/bedtime/extensions", ChangeKind::Loosen, later)
            },
        ]
    );
    assert_eq!(store.latest_text().unwrap().as_deref(), Some("two"));
}

#[test]
fn an_m1_store_opens_with_its_settings_kept_aside() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/selflow.db");
    m1_store(&path);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.latest_text().unwrap(), None);
    assert_eq!(store.m1_text().unwrap().as_deref(), Some(M1_TEXT));
    let kinds: Vec<EventKind> = store
        .events_for(date(2026, 11, 18))
        .unwrap()
        .iter()
        .map(|event| event.kind)
        .collect();
    assert_eq!(kinds, [EventKind::ConfigSaved]);
    assert_eq!(
        store.night_log(&[date(2026, 11, 19)]).unwrap(),
        DayLog::default()
    );
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
fn a_started_window_only_ever_moves_its_start_earlier() {
    let store = Store::open_in_memory().unwrap();
    let window = windows_for(
        &parse(&no_jitter()).unwrap(),
        Seed(7),
        date(2026, 11, 19),
        &brt(),
    )
    .remove(0);
    store.record_started(&window).unwrap();
    let moved = |hour, minute| RuleWindow {
        label: "sleep".to_owned(),
        starts_at: at(19, hour, minute),
        ..window.clone()
    };
    store.record_started(&moved(22, 50)).unwrap();
    store.record_started(&moved(23, 10)).unwrap();
    assert_eq!(
        store.night_log(&[date(2026, 11, 19)]).unwrap().started,
        vec![RuleWindow {
            starts_at: at(19, 22, 50),
            ..window
        }]
    );
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
        DayLog::default()
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
fn orders_events_by_time_across_fractions() {
    let store = Store::open_in_memory().unwrap();
    let whole = at(19, 23, 0).timestamp();
    let half = whole + jiff::SignedDuration::from_millis(500);
    let event = |at, key: &str| Event {
        at,
        logical_date: date(2026, 11, 19),
        kind: EventKind::Shutdown,
        key: key.into(),
        detail: None,
    };
    store
        .record_event(&event(whole + jiff::SignedDuration::from_secs(1), "c"))
        .unwrap();
    store.record_event(&event(half, "b")).unwrap();
    store.record_event(&event(whole, "a")).unwrap();
    let keys: Vec<_> = store
        .events_for(date(2026, 11, 19))
        .unwrap()
        .into_iter()
        .map(|e| e.key)
        .collect();
    assert_eq!(keys, vec!["a", "b", "c"]);
}
