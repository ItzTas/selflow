mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use jiff::civil::date;
use selflow_core::config::parse;
use selflow_core::enforce::DayLog;
use selflow_core::plan::{RuleWindow, windows_for};
use selflow_core::timing::Seed;
use selflow_core::versions::{ChangeKind, StoredChange, StoredVersion};
use selflowd::store::{Announcement, BootState, Event, EventKind, Store, StoredLock};
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

fn announcement(
    rule_key: &str,
    starts_at: jiff::Timestamp,
    announced_at: jiff::Timestamp,
) -> Announcement {
    Announcement {
        rule_key: rule_key.to_owned(),
        starts_at,
        announced_at,
    }
}

fn lock(identity: &str) -> StoredLock {
    StoredLock {
        identity: identity.to_owned(),
        label: "Bedtime".to_owned(),
        logical_date: date(2026, 11, 18),
        test: false,
        ended: false,
        shutdown_on_screen: None,
        shut_down: false,
    }
}

fn empty_boot(state: &BootState) -> bool {
    state.announced.is_empty() && state.sent.is_empty() && state.shut_down.is_empty()
}

#[test]
fn an_announcement_keeps_its_first_instant_within_a_boot() {
    let store = Store::open_in_memory().unwrap();
    let starts_at = at(18, 2, 30).timestamp();
    let first = at(18, 2, 0).timestamp();
    let second = first + jiff::SignedDuration::from_secs(30);
    store
        .record_announcement("b1", &announcement("rule", starts_at, first))
        .unwrap();
    store
        .record_announcement("b1", &announcement("rule", starts_at, second))
        .unwrap();
    let state = store.boot_state("b1", date(2026, 11, 17)).unwrap();
    assert_eq!(
        state.announced,
        vec![announcement("rule", starts_at, first)]
    );
}

#[test]
fn replacing_an_announcement_overwrites_its_instant_for_the_boot() {
    let store = Store::open_in_memory().unwrap();
    let starts_at = at(18, 2, 30).timestamp();
    let first = at(18, 2, 0).timestamp();
    let replaced = at(18, 2, 50).timestamp();
    let later = replaced + jiff::SignedDuration::from_secs(30);
    store
        .record_announcement("b1", &announcement("rule", starts_at, first))
        .unwrap();
    store
        .replace_announcement("b1", &announcement("rule", starts_at, replaced))
        .unwrap();
    store
        .record_announcement("b1", &announcement("rule", starts_at, later))
        .unwrap();
    let state = store.boot_state("b1", date(2026, 11, 17)).unwrap();
    assert_eq!(
        state.announced,
        vec![announcement("rule", starts_at, replaced)]
    );
}

#[test]
fn per_boot_rows_stay_with_their_boot() {
    let store = Store::open_in_memory().unwrap();
    let instant = at(18, 2, 0).timestamp();
    store
        .record_announcement("b1", &announcement("rule", instant, instant))
        .unwrap();
    store.record_notice_sent("b1", "notice", instant).unwrap();
    store.record_shutdown_done("b1", "rule").unwrap();
    let same = store.boot_state("b1", date(2026, 11, 17)).unwrap();
    assert_eq!(same.announced.len(), 1);
    assert_eq!(same.sent, vec![("notice".to_owned(), instant)]);
    assert_eq!(same.shut_down, vec!["rule".to_owned()]);
    assert!(empty_boot(
        &store.boot_state("b2", date(2026, 11, 17)).unwrap()
    ));
}

#[test]
fn a_lock_keeps_its_end_state_across_boots_but_not_its_shutdown() {
    let store = Store::open_in_memory().unwrap();
    let saved = StoredLock {
        shutdown_on_screen: Some(at(18, 23, 30).timestamp()),
        shut_down: true,
        ..lock("lock")
    };
    store.save_lock("b1", &saved).unwrap();
    let next = store.boot_state("b2", date(2026, 11, 17)).unwrap();
    assert_eq!(
        next.locks,
        vec![StoredLock {
            shutdown_on_screen: None,
            shut_down: false,
            ..saved.clone()
        }]
    );
    let same = store.boot_state("b1", date(2026, 11, 17)).unwrap();
    assert_eq!(same.locks, vec![saved]);
}

#[test]
fn saving_a_lock_again_updates_it() {
    let store = Store::open_in_memory().unwrap();
    let first = StoredLock {
        shutdown_on_screen: Some(at(18, 23, 30).timestamp()),
        ..lock("lock")
    };
    let second = StoredLock {
        shutdown_on_screen: Some(at(18, 23, 31).timestamp()),
        ..lock("lock")
    };
    store.save_lock("b1", &first).unwrap();
    store.save_lock("b1", &second).unwrap();
    let state = store.boot_state("b1", date(2026, 11, 17)).unwrap();
    assert_eq!(state.locks, vec![second]);
}

#[test]
fn forgetting_other_boots_keeps_open_locks() {
    let store = Store::open_in_memory().unwrap();
    let instant = at(18, 2, 0).timestamp();
    for boot in ["b1", "b2"] {
        store
            .record_announcement(boot, &announcement("rule", instant, instant))
            .unwrap();
        store.record_notice_sent(boot, "notice", instant).unwrap();
        store.record_shutdown_done(boot, "rule").unwrap();
    }
    let open = lock("open");
    let old_ended = StoredLock {
        ended: true,
        logical_date: date(2026, 11, 10),
        ..lock("old")
    };
    store.save_lock("b1", &open).unwrap();
    store.save_lock("b1", &old_ended).unwrap();
    store.forget_other_boots("b2", date(2026, 11, 17)).unwrap();
    assert!(empty_boot(
        &store.boot_state("b1", date(2026, 11, 17)).unwrap()
    ));
    let kept = store.boot_state("b2", date(2026, 11, 17)).unwrap();
    assert_eq!(kept.announced.len(), 1);
    assert_eq!(kept.sent.len(), 1);
    assert_eq!(kept.shut_down.len(), 1);
    assert_eq!(kept.locks, vec![open]);
    let everything = store.boot_state("b2", date(2026, 11, 1)).unwrap();
    assert_eq!(everything.locks.len(), 1);
}
