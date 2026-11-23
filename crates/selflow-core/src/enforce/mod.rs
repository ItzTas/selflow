mod types;

use std::cmp;

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil};

use crate::config::{Action, Challenges, Config};
use crate::plan::{RuleWindow, windows_around};
use crate::timing::{Seed, fnv1a64};

pub use types::{
    ActiveLock, DayLog, Enforcement, Evaluation, GrantedException, GrantedExtension, Notice,
    NoticeKind, ShutdownDone,
};

pub fn evaluate(now: &ZonedDateTime, config: &Config, seed: Seed, log: &DayLog) -> Evaluation {
    let instant = now.timestamp();
    let windows = candidates(now, config, seed, log);
    let active = windows
        .iter()
        .find_map(|window| active_lock(window, log, instant));
    let notices: Vec<Notice> = windows
        .iter()
        .flat_map(|window| notices_for(window, log, seed, instant))
        .collect();
    let next_change_at = notices
        .iter()
        .map(|notice| notice.at)
        .chain(windows.iter().map(|window| window.starts_at.timestamp()))
        .chain(log.extensions.iter().map(|extension| extension.until))
        .chain(active.iter().flat_map(lock_moments))
        .filter(|moment| *moment > instant)
        .min();
    Evaluation {
        enforcement: active.map_or(Enforcement::Free, Enforcement::Locked),
        notices,
        next_change_at,
        shutdown: windows
            .iter()
            .find(|window| due_shutdown(window, log, instant))
            .cloned(),
    }
}

fn due_shutdown(window: &RuleWindow, log: &DayLog, now: Timestamp) -> bool {
    let Action::ShutDown { repeat } = window.action else {
        return false;
    };
    let done = log
        .shutdowns
        .iter()
        .any(|done| is_window(window, &done.rule_id, done.logical_date));
    window.starts_at.timestamp() <= now && now < window.ends_at.timestamp() && (repeat || !done)
}

fn candidates(now: &ZonedDateTime, config: &Config, seed: Seed, log: &DayLog) -> Vec<RuleWindow> {
    let instant = now.timestamp();
    let computed = windows_around(config, seed, now);
    let started = log.started.iter().map(|started| {
        computed
            .iter()
            .find(|window| same_window(window, started))
            .map_or_else(
                || started.clone(),
                |window| merged(started, window, instant),
            )
    });
    let fresh = computed
        .iter()
        .filter(|window| {
            !log.started
                .iter()
                .any(|started| same_window(window, started))
        })
        .cloned();
    let mut windows: Vec<RuleWindow> = started
        .chain(fresh)
        .filter(|window| !excepted(window, log))
        .collect();
    windows.sort_by_key(|window| window.starts_at.timestamp());
    windows
}

fn excepted(window: &RuleWindow, log: &DayLog) -> bool {
    log.exceptions
        .iter()
        .any(|exception| is_window(window, &exception.rule_id, exception.logical_date))
}

fn merged(started: &RuleWindow, computed: &RuleWindow, now: Timestamp) -> RuleWindow {
    let (kept, current) = (&started.challenges, &computed.challenges);
    RuleWindow {
        label: computed.label.clone(),
        starts_at: merged_start(started, computed, now),
        ends_at: cmp::max_by_key(&started.ends_at, &computed.ends_at, |at| at.timestamp()).clone(),
        notices: computed.notices.clone(),
        message: computed.message.clone(),
        action: stricter_action(started.action, computed.action),
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

fn merged_start(started: &RuleWindow, computed: &RuleWindow, now: Timestamp) -> ZonedDateTime {
    if started.starts_at.timestamp() <= now {
        return started.starts_at.clone();
    }
    cmp::min_by_key(&started.starts_at, &computed.starts_at, |at| at.timestamp()).clone()
}

fn stricter_action(started: Action, computed: Action) -> Action {
    match (started, computed) {
        (
            Action::Lock {
                shutdown_after: kept,
            },
            Action::Lock {
                shutdown_after: current,
            },
        ) => Action::Lock {
            shutdown_after: kept.into_iter().chain(current).min(),
        },
        (Action::ShutDown { repeat: kept }, Action::ShutDown { repeat: current }) => {
            Action::ShutDown {
                repeat: kept || current,
            }
        }
        _ => started,
    }
}

fn same_window(window: &RuleWindow, other: &RuleWindow) -> bool {
    is_window(window, &other.rule_id, other.logical_date)
}

fn is_window(window: &RuleWindow, rule_id: &str, logical_date: civil::Date) -> bool {
    window.rule_id == rule_id && window.logical_date == logical_date
}

fn extensions_of<'a>(
    window: &'a RuleWindow,
    log: &'a DayLog,
) -> impl Iterator<Item = &'a GrantedExtension> {
    log.extensions
        .iter()
        .filter(move |extension| is_window(window, &extension.rule_id, extension.logical_date))
}

fn running_extension<'a>(
    window: &'a RuleWindow,
    log: &'a DayLog,
    now: Timestamp,
) -> Option<&'a GrantedExtension> {
    extensions_of(window, log)
        .find(|extension| extension.granted_at <= now && now < extension.until)
}

fn active_lock(window: &RuleWindow, log: &DayLog, now: Timestamp) -> Option<ActiveLock> {
    let Action::Lock { shutdown_after } = window.action else {
        return None;
    };
    let starts_at = window.starts_at.timestamp();
    let ends_at = window.ends_at.timestamp();
    if now < starts_at || now >= ends_at || running_extension(window, log, now).is_some() {
        return None;
    }
    let locked_since = locked_since(window, log, now);
    let granted = extensions_of(window, log).count();
    Some(ActiveLock {
        window: window.clone(),
        locked_since,
        extension_available_at: later_by(locked_since, window.challenges.wait),
        extensions_left: window
            .challenges
            .extensions
            .saturating_sub(u32::try_from(granted).unwrap_or(u32::MAX)),
        shutdown_at: shutdown_after
            .map(|after| later_by(locked_since, after))
            .filter(|shutdown_at| *shutdown_at < ends_at),
    })
}

fn locked_since(window: &RuleWindow, log: &DayLog, now: Timestamp) -> Timestamp {
    extensions_of(window, log)
        .map(|extension| extension.until)
        .filter(|until| *until <= now)
        .fold(window.starts_at.timestamp(), Ord::max)
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

fn notices_for(window: &RuleWindow, log: &DayLog, seed: Seed, now: Timestamp) -> Vec<Notice> {
    let Some((moment, longest)) = next_moment(window, log, now) else {
        return Vec::new();
    };
    let key = window.key();
    let variant = draw_variant(seed, &key, moment);
    let kind = notice_kind(window.action);
    let message = chosen_message(window, seed);
    let locked = window.action.locks() && moment <= now;
    window
        .notices
        .iter()
        .filter(|offset| **offset < longest)
        .map(|offset| earlier_by(moment, *offset))
        .filter(|at| !locked || *at == moment)
        .map(|at| Notice {
            key: key.clone(),
            label: window.label.clone(),
            at,
            moment,
            variant,
            kind,
            message: message.clone(),
        })
        .collect()
}

fn next_moment(
    window: &RuleWindow,
    log: &DayLog,
    now: Timestamp,
) -> Option<(Timestamp, SignedDuration)> {
    let starts_at = window.starts_at.timestamp();
    let ends_at = window.ends_at.timestamp();
    if !window.action.locks() {
        return (now < ends_at).then_some((starts_at, SignedDuration::MAX));
    }
    if let Some(extension) = running_extension(window, log, now) {
        return (extension.until < ends_at)
            .then(|| (extension.until, extension.until - extension.granted_at));
    }
    if now < starts_at {
        return Some((starts_at, SignedDuration::MAX));
    }
    (now < ends_at).then(|| (locked_since(window, log, now), SignedDuration::MAX))
}

fn notice_kind(action: Action) -> NoticeKind {
    match action {
        Action::Lock { .. } => NoticeKind::Lock,
        Action::ShutDown { .. } => NoticeKind::ShutDown,
        Action::Nothing => NoticeKind::Reminder,
    }
}

fn chosen_message(window: &RuleWindow, seed: Seed) -> Option<String> {
    if window.message.is_empty() {
        return None;
    }
    let mut rng = seeded(seed, &window.key(), &window.logical_date.to_string());
    Some(window.message[rng.usize(..window.message.len())].clone())
}

fn draw_variant(seed: Seed, key: &str, moment: Timestamp) -> u8 {
    seeded(seed, key, &moment.to_string()).u8(1..=3)
}

fn seeded(seed: Seed, key: &str, salt: &str) -> fastrand::Rng {
    let mut bytes = seed.0.to_le_bytes().to_vec();
    bytes.extend_from_slice(key.as_bytes());
    bytes.extend_from_slice(salt.as_bytes());
    fastrand::Rng::with_seed(fnv1a64(&bytes))
}

fn later_by(at: Timestamp, duration: SignedDuration) -> Timestamp {
    at.checked_add(duration).unwrap_or(Timestamp::MAX)
}

fn earlier_by(at: Timestamp, duration: SignedDuration) -> Timestamp {
    at.checked_sub(duration).unwrap_or(Timestamp::MIN)
}
