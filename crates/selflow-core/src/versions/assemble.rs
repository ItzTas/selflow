use std::collections::{BTreeSet, HashMap, HashSet};

use jiff::{
    Timestamp,
    civil::{Date, Weekday},
};

use super::types::{History, SavedConfig, exception_unit, rule_unit};
use crate::config::{Config, DAYS, Exception, Rule, RuleSet};

type CopyField = fn(&mut Rule, &Rule);

struct InForce<'h> {
    versions: HashMap<i64, &'h Config>,
    first: &'h Config,
    latest: &'h Config,
    sources: HashMap<&'h str, i64>,
}

pub fn config_at(history: &History, at: Timestamp) -> Config {
    let saved: Vec<&SavedConfig> = history
        .saved
        .iter()
        .filter(|version| version.saved_at <= at)
        .collect();
    let (Some(first), Some(latest)) = (saved.first(), saved.last()) else {
        return Config::empty();
    };
    let versions: HashMap<i64, &Config> = saved
        .iter()
        .map(|version| (version.id, &version.config))
        .collect();
    let mut sources: HashMap<&str, i64> = HashMap::new();
    for stored in history
        .changes
        .iter()
        .filter(|stored| stored.effective_at <= at && versions.contains_key(&stored.version_id))
    {
        let source = sources.entry(&stored.unit).or_insert(stored.version_id);
        *source = (*source).max(stored.version_id);
    }
    InForce {
        versions,
        first: &first.config,
        latest: &latest.config,
        sources,
    }
    .config()
}

impl<'h> InForce<'h> {
    fn config(&self) -> Config {
        Config {
            language: self.source("language").language.clone(),
            day_starts_at: self.source("day_starts_at").day_starts_at,
            loosening_delay: self.source("loosening_delay").loosening_delay,
            rule_sets: DAYS
                .iter()
                .filter_map(|(_, day)| self.rule_set(*day))
                .collect(),
            exceptions: self.exceptions(),
        }
    }

    fn source(&self, unit: &str) -> &'h Config {
        self.sources
            .get(unit)
            .and_then(|source| self.versions.get(source))
            .copied()
            .unwrap_or(self.first)
    }

    fn exceptions(&self) -> Vec<Exception> {
        let dates: BTreeSet<Date> = self
            .versions
            .values()
            .flat_map(|config| config.exceptions.iter().map(|exception| exception.date))
            .collect();
        dates
            .into_iter()
            .filter_map(|date| {
                self.source(&exception_unit(date))
                    .exception_on(date)
                    .cloned()
            })
            .collect()
    }

    fn rule_set(&self, day: Weekday) -> Option<RuleSet> {
        let candidates: BTreeSet<&str> = self
            .versions
            .values()
            .flat_map(|config| config.rules_on(day).iter().map(|rule| rule.id.as_str()))
            .collect();
        let latest = self.latest.rules_on(day);
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
        let named = self.latest.rule_set_for(day).or_else(|| {
            self.source(&rule_unit(day, &first.id, "presence"))
                .rule_set_for(day)
        })?;
        Some(RuleSet {
            name: named.name.clone(),
            days: HashSet::from([day]),
            rules,
        })
    }

    fn rule(&self, day: Weekday, id: &str) -> Option<Rule> {
        let base = rule_on(self.source(&rule_unit(day, id, "presence")), day, id)?;
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
            let from = rule_on(self.source(&rule_unit(day, id, field)), day, id).unwrap_or(base);
            copy(&mut rule, from);
        }
        Some(rule)
    }
}

fn rule_on<'c>(config: &'c Config, day: Weekday, id: &str) -> Option<&'c Rule> {
    config.rules_on(day).iter().find(|rule| rule.id == id)
}
