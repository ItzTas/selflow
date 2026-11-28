use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use jiff::{
    SignedDuration,
    civil::{self, Time, Weekday},
};
use toml::Spanned;

use super::constants::{
    AGENT_LEVELS, DAYS, DEFAULT_ALLOWED_COMMANDS, DEFAULT_CHALLENGES, DEFAULT_DAY_STARTS_AT,
    DEFAULT_LANGUAGE, DEFAULT_LOOSENING_DELAY, LONGEST_RULE, SHUTDOWN_REPEATS, SUPPORTED_LANGUAGES,
};
use super::day_tag;
use super::error::ConfigError;
use super::raw::{
    RawAgent, RawAgentStep, RawChallenges, RawConfig, RawException, RawRule, RawRuleSet,
};
use super::types::{
    Action, AgentLevel, AgentStep, Challenges, Config, Exception, Rule, RuleEnd, RuleSet,
};
use crate::timing::{TimingError, parse_clock, parse_duration, parse_offset};

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
    locks: bool,
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
        let exceptions = self.exceptions(&raw.exception, &raw.rule_set);
        let allowed_commands = self.allowed_commands(raw.agent.as_ref());
        Some(Config {
            language: language?,
            day_starts_at: day_starts_at?,
            loosening_delay: loosening_delay?,
            rule_sets: rule_sets?,
            exceptions: exceptions?,
            agent_allowed_commands: allowed_commands?,
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
        let challenges = self.challenges(&set.challenges, &DEFAULT_CHALLENGES);
        let inherited = challenges.clone().unwrap_or(DEFAULT_CHALLENGES);
        self.duplicate_rules(set);
        let (rules, windows): (Vec<_>, Vec<_>) = set
            .rule
            .iter()
            .map(|rule| self.rule(rule, context, &inherited))
            .unzip();
        if let Some(day_starts_at) = context.day_starts_at {
            let locks: Vec<_> = windows
                .into_iter()
                .flatten()
                .filter(|window| window.locks)
                .collect();
            self.overlaps(set, &days, &locks, day_starts_at);
        }
        let rules: Option<Vec<Rule>> = rules.into_iter().collect();
        Some(RuleSet {
            name: set.name.get_ref().clone(),
            days,
            rules: challenges.and(rules)?,
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

    fn challenges(&mut self, raw: &RawChallenges, inherited: &Challenges) -> Option<Challenges> {
        let wait = self.duration_or(raw.wait.as_ref(), inherited.wait);
        let extension_length =
            self.duration_or(raw.extension_length.as_ref(), inherited.extension_length);
        Some(Challenges {
            wait: wait?,
            reason: raw
                .reason
                .as_ref()
                .map_or(inherited.reason, |reason| *reason.get_ref()),
            phrase: raw.phrase.as_ref().map_or_else(
                || inherited.phrase.clone(),
                |phrase| Some(phrase.get_ref().clone()).filter(|phrase| !phrase.is_empty()),
            ),
            extensions: raw
                .extensions
                .as_ref()
                .map_or(inherited.extensions, |extensions| *extensions.get_ref()),
            extension_length: extension_length?,
        })
    }

    fn duplicate_rules(&mut self, set: &RawRuleSet) {
        let mut ids = HashSet::new();
        for rule in &set.rule {
            if ids.insert(rule.id.get_ref()) {
                continue;
            }
            let args = vec![
                ("id", rule.id.get_ref().clone()),
                ("set", set.name.get_ref().clone()),
            ];
            self.report(&rule.id, "config-duplicate-rule", args);
        }
    }

    fn rule<'r>(
        &mut self,
        raw: &'r RawRule,
        context: &Context,
        inherited: &Challenges,
    ) -> (Option<Rule>, Option<Window<'r>>) {
        let start = self.moment(&raw.start, context);
        let jitter = self.jitter(raw.start_jitter.as_ref());
        let end = self.end(raw, context, jitter);
        let window = start
            .zip(end)
            .and_then(|(start, end)| self.window(raw, start, end, jitter, context.day_starts_at));
        let rule = self.rule_with(raw, inherited, start, jitter, end);
        (rule, window)
    }

    fn rule_with(
        &mut self,
        raw: &RawRule,
        inherited: &Challenges,
        start: Option<Time>,
        start_jitter: Option<SignedDuration>,
        end: Option<RuleEnd>,
    ) -> Option<Rule> {
        let notices = self.notices(raw.notices.as_deref());
        let message = self.message(raw, raw.message.as_ref());
        let action = self.action(raw);
        let challenges = self.challenges(&raw.challenges, inherited);
        let agent = self.agent_steps(raw);
        Some(Rule {
            id: raw.id.get_ref().clone(),
            label: raw.label.as_ref().unwrap_or(&raw.id).get_ref().clone(),
            start: start?,
            start_jitter: start_jitter?,
            end: end?,
            notices: notices?,
            message: message?,
            action: action?,
            challenges: challenges?,
            agent: agent?,
        })
    }

    fn end(
        &mut self,
        raw: &RawRule,
        context: &Context,
        jitter: Option<SignedDuration>,
    ) -> Option<RuleEnd> {
        let end = raw.end.as_ref().map(|end| self.moment(end, context));
        let length = raw
            .length
            .as_ref()
            .map(|length| self.length(raw, length, jitter));
        if let (Some(_), Some(value)) = (&raw.end, &raw.length) {
            self.report(value, "config-end-and-length", id_arg(raw));
            return None;
        }
        match (end, length) {
            (Some(end), _) => end.map(RuleEnd::At),
            (_, Some(length)) => length.map(RuleEnd::After),
            (None, None) => Some(RuleEnd::DayEnd),
        }
    }

    fn length(
        &mut self,
        raw: &RawRule,
        value: &Spanned<String>,
        jitter: Option<SignedDuration>,
    ) -> Option<SignedDuration> {
        let length = self.duration(value)?;
        if length.saturating_add(jitter.unwrap_or_default()) < LONGEST_RULE {
            return Some(length);
        }
        self.report(value, "config-too-long", id_arg(raw));
        None
    }

    fn window<'r>(
        &mut self,
        raw: &'r RawRule,
        start: Time,
        end: RuleEnd,
        jitter: Option<SignedDuration>,
        day_starts_at: Option<Time>,
    ) -> Option<Window<'r>> {
        let length = window_length(start, end, day_starts_at)?;
        if let Some(value) = raw.end.as_ref().filter(|_| length.is_zero()) {
            self.report(value, "config-empty-window", id_arg(raw));
            return None;
        }
        if let Some(value) = &raw.start_jitter
            && jitter.is_some_and(|jitter| jitter >= length)
        {
            self.report(value, "config-jitter-too-long", id_arg(raw));
        }
        Some(Window {
            id: raw.id.get_ref(),
            at: &raw.start,
            start,
            length,
            locks: locks(raw),
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

    fn notices(
        &mut self,
        values: Option<&[Spanned<String>]>,
    ) -> Option<Option<Vec<SignedDuration>>> {
        let Some(values) = values else {
            return Some(None);
        };
        let parsed: Vec<_> = values.iter().map(|value| self.offset(value)).collect();
        let mut notices: Vec<_> = parsed.into_iter().collect::<Option<_>>()?;
        notices.sort_by_key(|notice| Reverse(*notice));
        Some(Some(notices))
    }

    fn agent_steps(&mut self, raw: &RawRule) -> Option<Vec<AgentStep>> {
        let steps: Vec<_> = raw
            .agent
            .iter()
            .map(|step| self.agent_step(raw, step))
            .collect();
        steps.into_iter().collect()
    }

    fn agent_step(&mut self, raw: &RawRule, step: &RawAgentStep) -> Option<AgentStep> {
        let before = self.before(raw, &step.before);
        let level = self.level(raw, &step.level);
        let message = self.message(raw, step.message.as_ref());
        let folders = self.folders(raw, step.folders.as_deref());
        Some(AgentStep {
            before: before?,
            level: level?,
            message: message?,
            folders: folders?,
            unattended: step
                .unattended
                .as_ref()
                .is_some_and(|unattended| *unattended.get_ref()),
        })
    }

    fn before(&mut self, raw: &RawRule, value: &Spanned<String>) -> Option<SignedDuration> {
        let before = self.offset(value)?;
        if before < LONGEST_RULE {
            return Some(before);
        }
        self.report(value, "config-agent-too-early", id_value_args(raw, value));
        None
    }

    fn level(&mut self, raw: &RawRule, value: &Spanned<String>) -> Option<AgentLevel> {
        let level = AGENT_LEVELS
            .iter()
            .find(|(name, _)| *name == value.get_ref().as_str())
            .map(|(_, level)| *level);
        if level.is_none() {
            self.report(value, "config-agent-bad-level", id_value_args(raw, value));
        }
        level
    }

    fn folders(
        &mut self,
        raw: &RawRule,
        values: Option<&[Spanned<String>]>,
    ) -> Option<Vec<String>> {
        let values = values.unwrap_or_default();
        let checked: Vec<_> = values.iter().map(|value| self.folder(raw, value)).collect();
        checked.into_iter().collect()
    }

    fn folder(&mut self, raw: &RawRule, value: &Spanned<String>) -> Option<String> {
        let folder = value.get_ref();
        if folder == "~" || folder.starts_with("~/") || Path::new(folder).is_absolute() {
            return Some(folder.clone());
        }
        self.report(value, "config-agent-bad-folder", id_value_args(raw, value));
        None
    }

    fn allowed_commands(&mut self, raw: Option<&RawAgent>) -> Option<Vec<String>> {
        let Some(values) = raw.and_then(|agent| agent.allowed_commands.as_ref()) else {
            return Some(
                DEFAULT_ALLOWED_COMMANDS
                    .iter()
                    .map(|command| (*command).to_owned())
                    .collect(),
            );
        };
        let checked: Vec<_> = values.iter().map(|value| self.command(value)).collect();
        checked.into_iter().collect()
    }

    fn command(&mut self, value: &Spanned<String>) -> Option<String> {
        let command = value.get_ref().trim();
        if command.is_empty() {
            self.report(value, "config-agent-empty-command", Vec::new());
            return None;
        }
        Some(command.to_owned())
    }

    fn message(
        &mut self,
        raw: &RawRule,
        value: Option<&Spanned<toml::Value>>,
    ) -> Option<Vec<String>> {
        let Some(value) = value else {
            return Some(Vec::new());
        };
        let Some(texts) = texts(value.get_ref()) else {
            self.report(value, "config-bad-message", id_arg(raw));
            return None;
        };
        if texts.iter().any(|text| text.trim().is_empty()) {
            self.report(value, "config-empty-message", id_arg(raw));
            return None;
        }
        Some(texts)
    }

    fn action(&mut self, raw: &RawRule) -> Option<Action> {
        let shutdown_after = self.optional_duration(raw.shutdown_after.as_ref());
        match raw.lock.as_ref().filter(|lock| *lock.get_ref()) {
            Some(lock) => self.lock(raw, lock, shutdown_after),
            None => self.without_lock(raw),
        }
    }

    fn lock(
        &mut self,
        raw: &RawRule,
        lock: &Spanned<bool>,
        shutdown_after: Option<Option<SignedDuration>>,
    ) -> Option<Action> {
        let has_end = raw.end.is_some() || raw.length.is_some();
        if !has_end {
            self.report(lock, "config-lock-needs-end", id_arg(raw));
        }
        if let Some(shutdown) = &raw.shutdown {
            self.report(shutdown, "config-shutdown-with-lock", id_arg(raw));
        }
        let shutdown_after = shutdown_after?;
        (has_end && raw.shutdown.is_none()).then_some(Action::Lock { shutdown_after })
    }

    fn without_lock(&mut self, raw: &RawRule) -> Option<Action> {
        let action = raw
            .shutdown
            .as_ref()
            .map_or(Some(Action::Nothing), |value| self.shutdown(value));
        let Some(value) = &raw.shutdown_after else {
            return action;
        };
        self.report(value, "config-shutdown-after-needs-lock", id_arg(raw));
        None
    }

    fn shutdown(&mut self, value: &Spanned<String>) -> Option<Action> {
        let repeat = SHUTDOWN_REPEATS
            .iter()
            .find(|(name, _)| *name == value.get_ref().as_str())
            .map(|(_, repeat)| *repeat);
        if repeat.is_none() {
            self.report(value, "config-bad-shutdown", value_arg(value));
        }
        repeat.map(|repeat| Action::ShutDown { repeat })
    }

    fn exceptions(&mut self, raw: &[RawException], sets: &[RawRuleSet]) -> Option<Vec<Exception>> {
        let mut dates = HashSet::new();
        let mut exceptions = Vec::new();
        for exception in raw {
            exceptions.push(self.exception(exception, sets, &mut dates));
        }
        exceptions.into_iter().collect()
    }

    fn exception(
        &mut self,
        raw: &RawException,
        sets: &[RawRuleSet],
        dates: &mut HashSet<civil::Date>,
    ) -> Option<Exception> {
        let date = self.date(&raw.date)?;
        if !dates.insert(date) {
            let args = vec![("date", raw.date.get_ref().clone())];
            self.report(&raw.date, "config-duplicate-exception", args);
        }
        let known = rule_ids_on(sets, date.weekday());
        for rule in raw
            .rules
            .iter()
            .filter(|rule| !known.contains(rule.get_ref().as_str()))
        {
            let args = vec![
                ("date", raw.date.get_ref().clone()),
                ("rule", rule.get_ref().clone()),
            ];
            self.report(rule, "config-exception-unknown-rule", args);
        }
        Some(Exception {
            date,
            rules: raw
                .rules
                .iter()
                .map(|rule| rule.get_ref().clone())
                .collect(),
        })
    }

    fn date(&mut self, value: &Spanned<String>) -> Option<civil::Date> {
        let date = value
            .get_ref()
            .parse::<civil::Date>()
            .ok()
            .filter(|date| date.to_string() == *value.get_ref());
        if date.is_none() {
            self.report(value, "config-bad-date", value_arg(value));
        }
        date
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
        self.timed(value, parse_duration)
    }

    fn offset(&mut self, value: &Spanned<String>) -> Option<SignedDuration> {
        self.timed(value, parse_offset)
    }

    fn timed(
        &mut self,
        value: &Spanned<String>,
        parse: fn(&str) -> Result<SignedDuration, TimingError>,
    ) -> Option<SignedDuration> {
        let id = match parse(value.get_ref()) {
            Ok(duration) => return Some(duration),
            Err(TimingError::Invalid(_)) => "config-bad-duration",
            Err(TimingError::NotPositive(_)) => "config-not-positive",
            Err(TimingError::Negative(_)) => "config-negative-offset",
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

fn window_length(start: Time, end: RuleEnd, day_starts_at: Option<Time>) -> Option<SignedDuration> {
    match end {
        RuleEnd::At(end) => Some(minutes_after(start, end)),
        RuleEnd::After(length) => Some(length),
        RuleEnd::DayEnd => day_starts_at.map(|day_starts_at| {
            SignedDuration::from_hours(24) - minutes_after(day_starts_at, start)
        }),
    }
}

fn texts(value: &toml::Value) -> Option<Vec<String>> {
    match value {
        toml::Value::String(text) => Some(vec![text.clone()]),
        toml::Value::Array(items) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => None,
    }
}

fn rule_ids_on(sets: &[RawRuleSet], day: Weekday) -> HashSet<&str> {
    sets.iter()
        .find(|set| set.days.iter().any(|value| value.get_ref() == day_tag(day)))
        .map(|set| {
            set.rule
                .iter()
                .map(|rule| rule.id.get_ref().as_str())
                .collect()
        })
        .unwrap_or_default()
}

fn locks(raw: &RawRule) -> bool {
    raw.lock.as_ref().is_some_and(|lock| *lock.get_ref())
}

fn minutes_after(from: Time, to: Time) -> SignedDuration {
    SignedDuration::from_mins(to.duration_since(from).as_mins().rem_euclid(24 * 60))
}

fn id_arg(raw: &RawRule) -> Args {
    vec![("id", raw.id.get_ref().clone())]
}

fn id_value_args(raw: &RawRule, value: &Spanned<String>) -> Args {
    vec![
        ("id", raw.id.get_ref().clone()),
        ("value", value.get_ref().clone()),
    ]
}

fn value_arg(value: &Spanned<String>) -> Args {
    vec![("value", value.get_ref().clone())]
}
