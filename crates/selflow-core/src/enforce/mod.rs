mod types;

use std::cmp;

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil};

use crate::config::{Challenges, Config};
use crate::plan::{LockWindow, windows_around};
use crate::timing::{Seed, fnv1a64};

pub use types::{ActiveLock, Enforcement, Evaluation, GrantedExtension, NightLog, Warning};

pub fn evaluate(now: &ZonedDateTime, config: &Config, seed: Seed, log: &NightLog) -> Evaluation {
    let instant = now.timestamp();
    let windows = candidates(now, config, seed, log);
    let active = windows
        .iter()
        .find_map(|window| active_lock(window, log, instant));
    let warnings: Vec<Warning> = windows
        .iter()
        .flat_map(|window| warnings_for(window, log, seed, instant))
        .collect();
    let next_change_at = warnings
        .iter()
        .map(|warning| warning.at)
        .chain(windows.iter().map(|window| window.starts_at.timestamp()))
        .chain(log.extensions.iter().map(|extension| extension.until))
        .chain(active.iter().flat_map(lock_moments))
        .filter(|moment| *moment > instant)
        .min();
    Evaluation {
        enforcement: active.map_or(Enforcement::Free, Enforcement::Locked),
        warnings,
        next_change_at,
    }
}

fn candidates(now: &ZonedDateTime, config: &Config, seed: Seed, log: &NightLog) -> Vec<LockWindow> {
    let computed = windows_around(config, seed, now);
    let started = log.started.iter().map(|started| {
        computed
            .iter()
            .find(|window| same_window(window, started))
            .map_or_else(|| started.clone(), |window| stricter(started, window))
    });
    let fresh = computed
        .iter()
        .filter(|window| {
            !log.started
                .iter()
                .any(|started| same_window(window, started))
        })
        .cloned();
    let mut windows: Vec<LockWindow> = started.chain(fresh).collect();
    windows.sort_by_key(|window| window.starts_at.timestamp());
    windows
}

fn stricter(started: &LockWindow, computed: &LockWindow) -> LockWindow {
    let (kept, current) = (&started.challenges, &computed.challenges);
    LockWindow {
        label: computed.label.clone(),
        ends_at: cmp::max_by_key(&started.ends_at, &computed.ends_at, |at| at.timestamp()).clone(),
        warnings: computed.warnings.clone(),
        shutdown_after: started
            .shutdown_after
            .into_iter()
            .chain(computed.shutdown_after)
            .min(),
        challenges: Challenges {
            wait: kept.wait.max(current.wait),
            reason: kept.reason || current.reason,
            phrase: kept.phrase.clone().or_else(|| current.phrase.clone()),
            extensions: kept.extensions.min(current.extensions),
            extension_length: kept.extension_length.min(current.extension_length),
        },
        ..started.clone()
    }
}

fn same_window(window: &LockWindow, other: &LockWindow) -> bool {
    is_window(window, &other.rule_set, &other.rule_id, other.logical_date)
}

fn is_window(
    window: &LockWindow,
    rule_set: &str,
    rule_id: &str,
    logical_date: civil::Date,
) -> bool {
    window.rule_set == rule_set && window.rule_id == rule_id && window.logical_date == logical_date
}

fn extensions_of<'a>(
    window: &'a LockWindow,
    log: &'a NightLog,
) -> impl Iterator<Item = &'a GrantedExtension> {
    log.extensions.iter().filter(move |extension| {
        is_window(
            window,
            &extension.rule_set,
            &extension.rule_id,
            extension.logical_date,
        )
    })
}

fn running_extension<'a>(
    window: &'a LockWindow,
    log: &'a NightLog,
    now: Timestamp,
) -> Option<&'a GrantedExtension> {
    extensions_of(window, log)
        .find(|extension| extension.granted_at <= now && now < extension.until)
}

fn active_lock(window: &LockWindow, log: &NightLog, now: Timestamp) -> Option<ActiveLock> {
    let starts_at = window.starts_at.timestamp();
    let ends_at = window.ends_at.timestamp();
    if now < starts_at || now >= ends_at || running_extension(window, log, now).is_some() {
        return None;
    }
    let locked_since = extensions_of(window, log)
        .map(|extension| extension.until)
        .filter(|until| *until <= now)
        .fold(starts_at, Ord::max);
    let granted = log
        .extensions
        .iter()
        .filter(|extension| {
            extension.rule_set == window.rule_set && extension.logical_date == window.logical_date
        })
        .count();
    Some(ActiveLock {
        window: window.clone(),
        locked_since,
        extension_available_at: later_by(locked_since, window.challenges.wait),
        extensions_left: window
            .challenges
            .extensions
            .saturating_sub(u32::try_from(granted).unwrap_or(u32::MAX)),
        shutdown_at: window
            .shutdown_after
            .map(|after| later_by(locked_since, after))
            .filter(|shutdown_at| *shutdown_at < ends_at),
    })
}

fn lock_moments(lock: &ActiveLock) -> impl Iterator<Item = Timestamp> {
    [
        Some(lock.extension_available_at),
        lock.shutdown_at,
        Some(lock.window.ends_at.timestamp()),
    ]
    .into_iter()
    .flatten()
}

fn warnings_for(window: &LockWindow, log: &NightLog, seed: Seed, now: Timestamp) -> Vec<Warning> {
    let Some((lock_at, longest)) = next_lock_moment(window, log, now) else {
        return Vec::new();
    };
    let key = window.key();
    let variant = draw_variant(seed, &key, lock_at);
    window
        .warnings
        .iter()
        .filter(|offset| **offset < longest)
        .map(|offset| Warning {
            key: key.clone(),
            label: window.label.clone(),
            at: earlier_by(lock_at, *offset),
            lock_at,
            variant,
        })
        .collect()
}

fn next_lock_moment(
    window: &LockWindow,
    log: &NightLog,
    now: Timestamp,
) -> Option<(Timestamp, SignedDuration)> {
    let starts_at = window.starts_at.timestamp();
    let Some(extension) = running_extension(window, log, now) else {
        return (now < starts_at).then_some((starts_at, SignedDuration::MAX));
    };
    (extension.until < window.ends_at.timestamp())
        .then(|| (extension.until, extension.until - extension.granted_at))
}

fn draw_variant(seed: Seed, key: &str, lock_at: Timestamp) -> u8 {
    let mut bytes = seed.0.to_le_bytes().to_vec();
    bytes.extend_from_slice(key.as_bytes());
    bytes.extend_from_slice(lock_at.to_string().as_bytes());
    fastrand::Rng::with_seed(fnv1a64(&bytes)).u8(1..=3)
}

fn later_by(at: Timestamp, duration: SignedDuration) -> Timestamp {
    at.checked_add(duration).unwrap_or(Timestamp::MAX)
}

fn earlier_by(at: Timestamp, duration: SignedDuration) -> Timestamp {
    at.checked_sub(duration).unwrap_or(Timestamp::MIN)
}
