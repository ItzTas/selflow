use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use jiff::{
    SignedDuration,
    civil::{Time, Weekday},
};
use toml::Spanned;

use super::constants::{
    DAYS, DEFAULT_DAY_STARTS_AT, DEFAULT_EXTENSION_LENGTH, DEFAULT_EXTENSIONS, DEFAULT_LANGUAGE,
    DEFAULT_LOOSENING_DELAY, DEFAULT_REASON, DEFAULT_WAIT, DEFAULT_WARNINGS, SUPPORTED_LANGUAGES,
};
use super::error::ConfigError;
use super::raw::{RawChallenges, RawConfig, RawLock, RawRuleSet};
use super::types::{Challenges, Config, LockRule, RuleSet};
use crate::timing::{TimingError, parse_clock, parse_duration};

type Args = Vec<(&'static str, String)>;

struct Context<'r> {
    anchors: HashMap<&'r str, Option<Time>>,
    day_starts_at: Option<Time>,
}

struct Window<'r> {
    id: &'r str,
    at: &'r Spanned<String>,
    start: Time,
    length: SignedDuration,
}

impl Window<'_> {
    fn overlaps(&self, other: &Window, day_starts_at: Time, shift: SignedDuration) -> bool {
        let start = minutes_after(day_starts_at, self.start);
        let other_start = minutes_after(day_starts_at, other.start) + shift;
        start < other_start + other.length && other_start < start + self.length
    }
}

struct Check<'t> {
    text: &'t str,
    errors: Vec<ConfigError>,
}

pub(super) fn validate(raw: &RawConfig, text: &str) -> Result<Config, Vec<ConfigError>> {
    let mut check = Check {
        text,
        errors: Vec::new(),
    };
    let config = check.config(raw);
    check.finish(config)
}

impl Check<'_> {
    fn config(&mut self, raw: &RawConfig) -> Option<Config> {
        let language = self.language(raw.language.as_ref());
        let day_starts_at = self.clock_or(raw.day_starts_at.as_ref(), DEFAULT_DAY_STARTS_AT);
        let loosening_delay =
            self.duration_or(raw.loosening_delay.as_ref(), DEFAULT_LOOSENING_DELAY);
        let context = Context {
            anchors: raw
                .anchors
                .iter()
                .map(|(name, value)| (name.as_str(), self.clock(value)))
                .collect(),
            day_starts_at,
        };
        let rule_sets = self.rule_sets(&raw.rule_set, &context);
        Some(Config {
            language: language?,
            day_starts_at: day_starts_at?,
            loosening_delay: loosening_delay?,
            rule_sets: rule_sets?,
        })
    }

    fn finish(mut self, config: Option<Config>) -> Result<Config, Vec<ConfigError>> {
        self.errors.sort_by_key(|error| (error.line, error.column));
        match config {
            Some(config) if self.errors.is_empty() => Ok(config),
            _ => Err(self.errors),
        }
    }

    fn rule_sets(&mut self, raw: &[RawRuleSet], context: &Context) -> Option<Vec<RuleSet>> {
        let mut names = HashSet::new();
        let mut claimed = HashMap::new();
        let mut sets = Vec::new();
        for set in raw {
            if !names.insert(set.name.get_ref()) {
                let args = vec![("name", set.name.get_ref().clone())];
                self.report(&set.name, "config-duplicate-set", args);
            }
            sets.push(self.rule_set(set, context, &mut claimed));
        }
        sets.into_iter().collect()
    }

    fn rule_set<'r>(
        &mut self,
        set: &'r RawRuleSet,
        context: &Context,
        claimed: &mut HashMap<Weekday, &'r str>,
    ) -> Option<RuleSet> {
        let days = self.days(set, claimed);
        let challenges = self.challenges(&set.challenges);
        self.duplicate_locks(set);
        let (locks, windows): (Vec<_>, Vec<_>) =
            set.lock.iter().map(|lock| self.lock(lock, context)).unzip();
        if let Some(day_starts_at) = context.day_starts_at {
            let windows: Vec<_> = windows.into_iter().flatten().collect();
            self.overlaps(set, &days, &windows, day_starts_at);
        }
        Some(RuleSet {
            name: set.name.get_ref().clone(),
            days,
            challenges: challenges?,
            locks: locks.into_iter().collect::<Option<_>>()?,
        })
    }

    fn days<'r>(
        &mut self,
        set: &'r RawRuleSet,
        claimed: &mut HashMap<Weekday, &'r str>,
    ) -> HashSet<Weekday> {
        let mut days = HashSet::new();
        for value in &set.days {
            let Some(day) = self.day(value) else { continue };
            if !days.insert(day) {
                continue;
            }
            let Some(first) = claimed.get(&day).copied() else {
                claimed.insert(day, set.name.get_ref());
                continue;
            };
            let args = vec![
                ("day", value.get_ref().clone()),
                ("first", first.to_owned()),
                ("second", set.name.get_ref().clone()),
            ];
            self.report(value, "config-day-in-two-sets", args);
        }
        days
    }

    fn day(&mut self, value: &Spanned<String>) -> Option<Weekday> {
        let day = DAYS
            .iter()
            .find(|(name, _)| *name == value.get_ref().as_str())
            .map(|(_, day)| *day);
        if day.is_none() {
            self.report(value, "config-bad-day", value_arg(value));
        }
        day
    }

    fn challenges(&mut self, raw: &RawChallenges) -> Option<Challenges> {
        let wait = self.duration_or(raw.wait.as_ref(), DEFAULT_WAIT);
        let extension_length =
            self.duration_or(raw.extension_length.as_ref(), DEFAULT_EXTENSION_LENGTH);
        Some(Challenges {
            wait: wait?,
            reason: raw
                .reason
                .as_ref()
                .map_or(DEFAULT_REASON, |reason| *reason.get_ref()),
            phrase: raw
                .phrase
                .as_ref()
                .map(|phrase| phrase.get_ref().clone())
                .filter(|phrase| !phrase.is_empty()),
            extensions: raw
                .extensions
                .as_ref()
                .map_or(DEFAULT_EXTENSIONS, |extensions| *extensions.get_ref()),
            extension_length: extension_length?,
        })
    }

    fn duplicate_locks(&mut self, set: &RawRuleSet) {
        let mut ids = HashSet::new();
        for lock in &set.lock {
            if ids.insert(lock.id.get_ref()) {
                continue;
            }
            let args = vec![
                ("id", lock.id.get_ref().clone()),
                ("set", set.name.get_ref().clone()),
            ];
            self.report(&lock.id, "config-duplicate-lock", args);
        }
    }

    fn lock<'r>(
        &mut self,
        raw: &'r RawLock,
        context: &Context,
    ) -> (Option<LockRule>, Option<Window<'r>>) {
        let start = self.moment(&raw.start, context);
        let end = self.moment(&raw.end, context);
        let jitter = self.jitter(raw.start_jitter.as_ref());
        let warnings = self.warnings(raw.warnings.as_deref());
        let shutdown_after = self.optional_duration(raw.shutdown_after.as_ref());
        let window = start
            .zip(end)
            .and_then(|(start, end)| self.window(raw, start, end, jitter));
        let rule = match (start, end, jitter, warnings, shutdown_after) {
            (Some(start), Some(end), Some(start_jitter), Some(warnings), Some(shutdown_after)) => {
                Some(LockRule {
                    id: raw.id.get_ref().clone(),
                    label: raw.label.as_ref().unwrap_or(&raw.id).get_ref().clone(),
                    start,
                    start_jitter,
                    end,
                    warnings,
                    shutdown_after,
                })
            }
            _ => None,
        };
        (rule, window)
    }

    fn window<'r>(
        &mut self,
        lock: &'r RawLock,
        start: Time,
        end: Time,
        jitter: Option<SignedDuration>,
    ) -> Option<Window<'r>> {
        let length = minutes_after(start, end);
        if length.is_zero() {
            let args = vec![("id", lock.id.get_ref().clone())];
            self.report(&lock.end, "config-empty-window", args);
            return None;
        }
        if let Some(value) = &lock.start_jitter
            && jitter.is_some_and(|jitter| jitter >= length)
        {
            let args = vec![("id", lock.id.get_ref().clone())];
            self.report(value, "config-jitter-too-long", args);
        }
        Some(Window {
            id: lock.id.get_ref(),
            at: &lock.start,
            start,
            length,
        })
    }

    fn overlaps(
        &mut self,
        set: &RawRuleSet,
        days: &HashSet<Weekday>,
        windows: &[Window],
        day_starts_at: Time,
    ) {
        let next_day_too = days.iter().any(|day| days.contains(&day.next()));
        let shifts = if next_day_too { -1..=1 } else { 0..=0 };
        for (index, later) in windows.iter().enumerate() {
            let overlapping = windows[..index].iter().filter(|earlier| {
                shifts.clone().any(|offset| {
                    earlier.overlaps(
                        later,
                        day_starts_at,
                        SignedDuration::from_hours(24 * offset),
                    )
                })
            });
            for earlier in overlapping {
                let args = vec![
                    ("first", earlier.id.to_owned()),
                    ("second", later.id.to_owned()),
                    ("set", set.name.get_ref().clone()),
                ];
                self.report(later.at, "config-locks-overlap", args);
            }
        }
    }

    fn warnings(&mut self, values: Option<&[Spanned<String>]>) -> Option<Vec<SignedDuration>> {
        let Some(values) = values else {
            return Some(DEFAULT_WARNINGS.to_vec());
        };
        let parsed: Vec<_> = values.iter().map(|value| self.duration(value)).collect();
        let mut warnings: Vec<_> = parsed.into_iter().collect::<Option<_>>()?;
        warnings.sort_by_key(|warning| Reverse(*warning));
        Some(warnings)
    }

    fn moment(&mut self, value: &Spanned<String>, context: &Context) -> Option<Time> {
        if value.get_ref().contains(':') {
            return self.clock(value);
        }
        let Some(anchor) = context.anchors.get(value.get_ref().as_str()) else {
            let args = vec![("name", value.get_ref().clone())];
            self.report(value, "config-unknown-anchor", args);
            return None;
        };
        *anchor
    }

    fn language(&mut self, value: Option<&Spanned<String>>) -> Option<String> {
        let Some(value) = value else {
            return Some(DEFAULT_LANGUAGE.to_owned());
        };
        if SUPPORTED_LANGUAGES.contains(&value.get_ref().as_str()) {
            return Some(value.get_ref().clone());
        }
        self.report(value, "config-unknown-language", value_arg(value));
        None
    }

    fn jitter(&mut self, value: Option<&Spanned<String>>) -> Option<SignedDuration> {
        let Some(value) = value else {
            return Some(SignedDuration::ZERO);
        };
        let zero = value
            .get_ref()
            .trim()
            .parse()
            .is_ok_and(|jitter: SignedDuration| jitter.is_zero());
        if zero {
            return Some(SignedDuration::ZERO);
        }
        self.duration(value)
    }

    fn optional_duration(
        &mut self,
        value: Option<&Spanned<String>>,
    ) -> Option<Option<SignedDuration>> {
        value.map_or(Some(None), |value| self.duration(value).map(Some))
    }

    fn duration_or(
        &mut self,
        value: Option<&Spanned<String>>,
        default: SignedDuration,
    ) -> Option<SignedDuration> {
        value.map_or(Some(default), |value| self.duration(value))
    }

    fn duration(&mut self, value: &Spanned<String>) -> Option<SignedDuration> {
        let id = match parse_duration(value.get_ref()) {
            Ok(duration) => return Some(duration),
            Err(TimingError::Invalid(_)) => "config-bad-duration",
            Err(TimingError::NotPositive(_)) => "config-not-positive",
        };
        self.report(value, id, value_arg(value));
        None
    }

    fn clock_or(&mut self, value: Option<&Spanned<String>>, default: Time) -> Option<Time> {
        value.map_or(Some(default), |value| self.clock(value))
    }

    fn clock(&mut self, value: &Spanned<String>) -> Option<Time> {
        let time = parse_clock(value.get_ref()).ok();
        if time.is_none() {
            self.report(value, "config-bad-time", value_arg(value));
        }
        time
    }

    fn report<T>(&mut self, value: &Spanned<T>, id: &'static str, args: Args) {
        self.errors
            .push(ConfigError::at(self.text, value.span(), id, args));
    }
}

fn minutes_after(from: Time, to: Time) -> SignedDuration {
    SignedDuration::from_mins(to.duration_since(from).as_mins().rem_euclid(24 * 60))
}

fn value_arg(value: &Spanned<String>) -> Args {
    vec![("value", value.get_ref().clone())]
}
