use std::collections::{BTreeSet, HashMap, HashSet};

use jiff::{
    Timestamp,
    civil::{Date, Weekday},
};

use super::just_before;
use super::types::{History, SavedConfig, exception_unit, rule_unit};
use crate::config::{Config, DAYS, Exception, Rule, RuleSet};

type CopyField = fn(&mut Rule, &Rule);

struct InForce<'h> {
    at: Timestamp,
    saved: Vec<&'h SavedConfig>,
    first: &'h SavedConfig,
    latest: &'h SavedConfig,
    versions: HashMap<i64, &'h SavedConfig>,
    changes: HashMap<&'h str, Vec<(Timestamp, i64)>>,
}

pub fn config_at(history: &History, at: Timestamp) -> Config {
    let saved: Vec<&SavedConfig> = history
        .saved
        .iter()
        .filter(|version| version.saved_at <= at)
        .collect();
    let (Some(&first), Some(&latest)) = (saved.first(), saved.last()) else {
        return Config::empty();
    };
    let versions: HashMap<i64, &SavedConfig> =
        saved.iter().map(|version| (version.id, *version)).collect();
    let mut changes: HashMap<&str, Vec<(Timestamp, i64)>> = HashMap::new();
    for stored in history
        .changes
        .iter()
        .filter(|stored| stored.effective_at <= at && versions.contains_key(&stored.version_id))
    {
        changes
            .entry(&stored.unit)
            .or_default()
            .push((stored.effective_at, stored.version_id));
    }
    InForce {
        at,
        saved,
        first,
        latest,
        versions,
        changes,
    }
    .config()
}

impl<'h> InForce<'h> {
    fn config(&self) -> Config {
        Config {
            language: self.source("language").config.language.clone(),
            day_starts_at: self.source("day_starts_at").config.day_starts_at,
            loosening_delay: self.source("loosening_delay").config.loosening_delay,
            rule_sets: DAYS
                .iter()
                .filter_map(|(_, day)| self.rule_set(*day))
                .collect(),
            exceptions: self.exceptions(),
        }
    }

    fn source(&self, unit: &str) -> &'h SavedConfig {
        self.source_at(unit, self.at).unwrap_or(self.first)
    }

    fn source_at(&self, unit: &str, time: Timestamp) -> Option<&'h SavedConfig> {
        let saved_by = |version: &&SavedConfig| version.saved_at <= time;
        self.changes
            .get(unit)
            .into_iter()
            .flatten()
            .filter(|(effective_at, _)| *effective_at <= time)
            .filter_map(|(_, id)| self.versions.get(id).copied().filter(saved_by))
            .max_by_key(|version| version.id)
            .or_else(|| self.saved.iter().copied().find(saved_by))
    }

    fn arrival(&self, day: Weekday, id: &str) -> Option<&'h SavedConfig> {
        let unit = rule_unit(day, id, "presence");
        let present = |time| {
            self.source_at(&unit, time)
                .is_some_and(|version| rule_on(&version.config, day, id).is_some())
        };
        let mut times: Vec<Timestamp> = self
            .changes
            .get(unit.as_str())
            .into_iter()
            .flatten()
            .map(|(time, _)| *time)
            .collect();
        times.sort_unstable();
        times.dedup();
        let arrived = times
            .into_iter()
            .rev()
            .find(|time| present(*time) && !present(just_before(*time)))?;
        self.source_at(&unit, arrived)
    }

    fn exceptions(&self) -> Vec<Exception> {
        let dates: BTreeSet<Date> = self
            .saved
            .iter()
            .flat_map(|version| {
                version
                    .config
                    .exceptions
                    .iter()
                    .map(|exception| exception.date)
            })
            .collect();
        dates
            .into_iter()
            .filter_map(|date| {
                self.source(&exception_unit(date))
                    .config
                    .exception_on(date)
                    .cloned()
            })
            .collect()
    }

    fn rule_set(&self, day: Weekday) -> Option<RuleSet> {
        let candidates: BTreeSet<&str> = self
            .saved
            .iter()
            .flat_map(|version| {
                version
                    .config
                    .rules_on(day)
                    .iter()
                    .map(|rule| rule.id.as_str())
            })
            .collect();
        let latest = self.latest.config.rules_on(day);
        let others = candidates
            .into_iter()
            .filter(|id| !latest.iter().any(|rule| rule.id == *id));
        let rules: Vec<Rule> = latest
            .iter()
            .map(|rule| rule.id.as_str())
            .chain(others)
            .filter_map(|id| self.rule(day, id))
            .collect();
        let first = rules.first()?;
        let named = self.latest.config.rule_set_for(day).or_else(|| {
            self.source(&rule_unit(day, &first.id, "presence"))
                .config
                .rule_set_for(day)
        })?;
        Some(RuleSet {
            name: named.name.clone(),
            days: HashSet::from([day]),
            rules,
        })
    }

    fn rule(&self, day: Weekday, id: &str) -> Option<Rule> {
        let base = rule_on(
            &self.source(&rule_unit(day, id, "presence")).config,
            day,
            id,
        )?;
        let arrival = self.arrival(day, id);
        let fields: [(&str, CopyField); 10] = [
            ("timing", |rule, from| {
                rule.start = from.start;
                rule.start_jitter = from.start_jitter;
                rule.end = from.end;
            }),
            ("action", |rule, from| rule.action = from.action),
            ("notices", |rule, from| {
                rule.notices.clone_from(&from.notices)
            }),
            ("message", |rule, from| {
                rule.message.clone_from(&from.message)
            }),
            ("label", |rule, from| rule.label.clone_from(&from.label)),
            ("wait", |rule, from| {
                rule.challenges.wait = from.challenges.wait
            }),
            ("reason", |rule, from| {
                rule.challenges.reason = from.challenges.reason;
            }),
            ("phrase", |rule, from| {
                rule.challenges.phrase.clone_from(&from.challenges.phrase);
            }),
            ("extensions", |rule, from| {
                rule.challenges.extensions = from.challenges.extensions;
            }),
            ("extension_length", |rule, from| {
                rule.challenges.extension_length = from.challenges.extension_length;
            }),
        ];
        let mut rule = base.clone();
        for (field, copy) in fields {
            let source = self.source(&rule_unit(day, id, field));
            let source = arrival
                .filter(|arrived| arrived.id > source.id)
                .unwrap_or(source);
            copy(&mut rule, rule_on(&source.config, day, id).unwrap_or(base));
        }
        Some(rule)
    }
}

fn rule_on<'c>(config: &'c Config, day: Weekday, id: &str) -> Option<&'c Rule> {
    config.rules_on(day).iter().find(|rule| rule.id == id)
}
