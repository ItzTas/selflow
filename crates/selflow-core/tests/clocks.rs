mod support;

use jiff::{
    SignedDuration, ZonedDateTime,
    civil::{Weekday, time},
};
use selflow_core::config::{Action, AgentLevel, AgentStep, Config, parse};
use selflow_core::versions::{History, Scope, config_at, plan_save, rule_since, shown, waiting};
use support::{VALID, at, history_of, no_jitter};

const LATER_BEDTIME: (&str, &str) = (r#"bedtime = "23:00""#, r#"bedtime = "01:00""#);
const MORE_EXTENSIONS: (&str, &str) = ("extensions = 1", "extensions = 3");
const NO_LOCK: (&str, &str) = ("lock = true\nshutdown_after = \"30m\"\n", "");
const NO_THURSDAY: (&str, &str) = (r#""wed", "thu"]"#, r#""wed"]"#);
const SLEEP_LABEL: (&str, &str) = (
    "id = \"bedtime\"\n",
    "id = \"bedtime\"\nlabel = \"Sleep\"\n",
);
const SHORT_DELAY: (&str, &str) = (r#"loosening_delay = "24h""#, r#"loosening_delay = "1h""#);

#[test]
fn putting_back_a_dropped_day_cancels_its_waiting_changes() {
    let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();
    let base = no_jitter();
    let more = base.replace(MORE_EXTENSIONS.0, MORE_EXTENSIONS.1);
    let more_without_thursday = more.replace(NO_THURSDAY.0, NO_THURSDAY.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 10, 0), more.as_str()),
        (at(18, 11, 0), more_without_thursday.as_str()),
        (at(18, 12, 0), base.as_str()),
    ]);
    for when in [at(19, 12, 0), at(25, 12, 0)] {
        let config = config_at(&history, when.timestamp());
        assert_eq!(rule(&config).challenges.extensions, 1);
        assert_eq!(config.rules_on(Weekday::Monday)[0].challenges.extensions, 1);
    }
    assert!(waiting(&history, at(18, 12, 0).timestamp()).is_empty());
}

#[test]
fn a_rule_brought_back_cancels_what_still_waited_from_before_its_removal() {
    let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();
    let base = no_jitter();
    let short = base.replace(SHORT_DELAY.0, SHORT_DELAY.1);
    let short_more = short.replace(MORE_EXTENSIONS.0, MORE_EXTENSIONS.1);
    let short_more_without_thursday = short_more.replace(NO_THURSDAY.0, NO_THURSDAY.1);
    let history = history_of(&[
        (at(17, 8, 0), base.as_str()),
        (at(17, 9, 0), short.as_str()),
        (at(18, 8, 0), short_more.as_str()),
        (at(18, 10, 0), short_more_without_thursday.as_str()),
        (at(18, 12, 0), short.as_str()),
    ]);
    assert_eq!(
        config_at(&history, at(18, 11, 30).timestamp()).rules_on(Weekday::Thursday),
        []
    );
    for when in [at(18, 12, 0), at(19, 12, 0)] {
        assert_eq!(
            rule(&config_at(&history, when.timestamp()))
                .challenges
                .extensions,
            1
        );
    }
    assert!(waiting(&history, at(18, 12, 0).timestamp()).is_empty());
}

#[test]
fn shown_names_each_day_once() {
    let base = no_jitter();
    let both = base
        .replace(LATER_BEDTIME.0, LATER_BEDTIME.1)
        .replace(MORE_EXTENSIONS.0, MORE_EXTENSIONS.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 10, 0), both.as_str()),
    ]);
    let entries = shown(&plan_save(
        &history,
        &parse(&base).unwrap(),
        at(18, 12, 0).timestamp(),
    ));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "change-canceled");
    assert_eq!(
        entries[0].args,
        [
            ("rule", "bedtime".to_owned()),
            ("days", "mon, tue, wed, thu, sun".to_owned()),
        ]
    );
}

#[test]
fn a_rule_brought_back_takes_its_fields_from_the_save_that_returns_it() {
    let base = no_jitter();
    let no_thursday = base.replace(NO_THURSDAY.0, NO_THURSDAY.1);
    let earlier = base.replace(r#"bedtime = "23:00""#, r#"bedtime = "21:00""#);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 9, 0), no_thursday.as_str()),
        (at(20, 10, 0), earlier.as_str()),
    ]);
    let starts = |when: ZonedDateTime, day| {
        config_at(&history, when.timestamp())
            .rules_on(day)
            .first()
            .map(|rule| rule.start)
    };
    assert_eq!(starts(at(19, 12, 0), Weekday::Thursday), None);
    for when in [at(20, 10, 0), at(21, 12, 0)] {
        assert_eq!(
            starts(when.clone(), Weekday::Thursday),
            Some(time(21, 0, 0, 0))
        );
        assert_eq!(starts(when, Weekday::Monday), Some(time(21, 0, 0, 0)));
    }
}

#[test]
fn a_rule_put_back_while_its_removal_waits_keeps_the_fields_of_that_save() {
    let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();
    let base = no_jitter();
    let no_thursday = base.replace(NO_THURSDAY.0, NO_THURSDAY.1);
    let back_as_sleep = base.replace(SLEEP_LABEL.0, SLEEP_LABEL.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 9, 0), no_thursday.as_str()),
        (at(18, 10, 0), back_as_sleep.as_str()),
    ]);
    assert_eq!(
        rule(&config_at(&history, at(18, 10, 0).timestamp())).label,
        "Sleep"
    );
    assert!(waiting(&history, at(18, 10, 0).timestamp()).is_empty());
}

#[test]
fn a_rule_put_back_while_its_removal_waits_keeps_its_loosening_on_its_clock() {
    let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();
    let base = no_jitter();
    let more = base.replace(MORE_EXTENSIONS.0, MORE_EXTENSIONS.1);
    let more_without_thursday = more.replace(NO_THURSDAY.0, NO_THURSDAY.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 10, 0), more.as_str()),
        (at(18, 11, 0), more_without_thursday.as_str()),
        (at(18, 12, 0), more.as_str()),
    ]);
    let extensions = |when: ZonedDateTime| {
        rule(&config_at(&history, when.timestamp()))
            .challenges
            .extensions
    };
    assert_eq!(extensions(at(18, 12, 0)), 1);
    assert_eq!(extensions(at(19, 10, 0)), 3);
    assert_eq!(extensions(at(19, 12, 0)), 3);
}

#[test]
fn a_canceled_change_holds_no_clock() {
    let base = no_jitter();
    let later = base.replace(LATER_BEDTIME.0, LATER_BEDTIME.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 10, 0), later.as_str()),
        (at(18, 12, 0), base.as_str()),
    ]);
    let changes = plan_save(&history, &parse(&later).unwrap(), at(18, 20, 0).timestamp());
    assert!(!changes.is_empty());
    assert!(
        changes
            .iter()
            .all(|timed| timed.change.id == "change-rule-starts-later"
                && timed.effective_at == at(19, 20, 0).timestamp())
    );
}

#[test]
fn a_tightening_and_a_loosening_in_one_save_keep_their_own_clocks() {
    let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();
    let base = no_jitter();
    let both = base
        .replace(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#)
        .replace("extensions = 1", "extensions = 3");
    let history = history_of(&[(at(18, 10, 0), &base), (at(18, 12, 0), &both)]);
    let now = config_at(&history, at(18, 12, 0).timestamp());
    assert_eq!(rule(&now).start, time(22, 0, 0, 0));
    assert_eq!(rule(&now).challenges.extensions, 1);
    assert_eq!(
        rule(&config_at(&history, at(19, 12, 0).timestamp()))
            .challenges
            .extensions,
        3
    );
}

#[test]
fn a_later_save_does_not_restart_a_waiting_change() {
    let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();
    let base = no_jitter();
    let more = base.replace("extensions = 1", "extensions = 3");
    let renamed = more.replace(
        "id = \"bedtime\"\n",
        "id = \"bedtime\"\nlabel = \"Sleep\"\n",
    );
    let history = history_of(&[
        (at(18, 10, 0), &base),
        (at(18, 12, 0), &more),
        (at(18, 20, 0), &renamed),
    ]);
    assert_eq!(
        rule(&config_at(&history, at(18, 20, 0).timestamp())).label,
        "Sleep"
    );
    assert_eq!(
        rule(&config_at(&history, at(19, 12, 0).timestamp()))
            .challenges
            .extensions,
        3
    );
}

#[test]
fn putting_back_the_value_in_force_cancels_the_waiting_change() {
    let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();
    let base = no_jitter();
    let later = base.replace(LATER_BEDTIME.0, LATER_BEDTIME.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 10, 0), later.as_str()),
    ]);
    let changes = plan_save(&history, &parse(&base).unwrap(), at(18, 12, 0).timestamp());
    assert!(!changes.is_empty());
    assert!(
        changes
            .iter()
            .all(|timed| timed.change.id == "change-canceled"
                && timed.effective_at == at(18, 12, 0).timestamp())
    );
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 10, 0), later.as_str()),
        (at(18, 12, 0), base.as_str()),
    ]);
    assert_eq!(
        rule(&config_at(&history, at(19, 11, 0).timestamp())).start,
        time(23, 0, 0, 0)
    );
}

#[test]
fn a_value_between_the_one_in_force_and_a_waiting_one_keeps_the_waiting_clock() {
    let base = no_jitter();
    let later = base.replace(LATER_BEDTIME.0, LATER_BEDTIME.1);
    let between = base.replace(LATER_BEDTIME.0, r#"bedtime = "00:00""#);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 10, 0), later.as_str()),
    ]);
    let changes = plan_save(
        &history,
        &parse(&between).unwrap(),
        at(18, 20, 0).timestamp(),
    );
    assert!(!changes.is_empty());
    assert!(
        changes
            .iter()
            .all(|timed| timed.change.id == "change-rule-starts-later"
                && timed.effective_at == at(19, 10, 0).timestamp())
    );
}

#[test]
fn a_loosening_waits_the_delay_in_force_even_when_the_file_shortens_it() {
    let base = no_jitter();
    let proposed = base
        .replace(LATER_BEDTIME.0, LATER_BEDTIME.1)
        .replace(SHORT_DELAY.0, SHORT_DELAY.1);
    let history = history_of(&[(at(18, 8, 0), base.as_str())]);
    let changes = plan_save(
        &history,
        &parse(&proposed).unwrap(),
        at(18, 10, 0).timestamp(),
    );
    assert!(
        changes
            .iter()
            .any(|timed| timed.change.id == "change-delay-shorter")
    );
    assert!(
        changes
            .iter()
            .all(|timed| timed.effective_at == at(19, 10, 0).timestamp())
    );
}

#[test]
fn the_first_save_takes_effect_at_once() {
    let text = no_jitter()
        .replace(r#"loosening_delay = "24h""#, r#"loosening_delay = "1h""#)
        .replace(r#"day_starts_at = "05:00""#, r#"day_starts_at = "04:00""#);
    let saved_at = at(18, 8, 0).timestamp();
    let changes = plan_save(&History::default(), &parse(&text).unwrap(), saved_at);
    let ids: Vec<&str> = changes.iter().map(|timed| timed.change.id).collect();
    assert!(ids.contains(&"change-delay-shorter"));
    assert!(ids.contains(&"change-day-start"));
    assert!(ids.contains(&"change-rule-added"));
    assert!(changes.iter().all(|timed| timed.effective_at == saved_at));
}

#[test]
fn a_lock_swapped_for_a_shutdown_waits_as_one_unit() {
    let base = no_jitter();
    let shutdown = base.replace(NO_LOCK.0, "shutdown = \"repeat\"\n");
    let history = history_of(&[(at(18, 8, 0), base.as_str())]);
    let changes = plan_save(
        &history,
        &parse(&shutdown).unwrap(),
        at(18, 12, 0).timestamp(),
    );
    let ids: Vec<&str> = changes.iter().map(|timed| timed.change.id).collect();
    assert!(ids.contains(&"change-lock-removed"));
    assert!(ids.contains(&"change-shutdown-at-start-added"));
    assert!(
        changes.iter().all(|timed| timed.change.field == "action"
            && timed.effective_at == at(19, 12, 0).timestamp())
    );
}

#[test]
fn rule_since_is_when_the_rule_last_came_into_force() {
    let relabeled = VALID.replace(
        "id = \"bedtime\"\n",
        "id = \"bedtime\"\nlabel = \"Sleep\"\n",
    );
    let sooner = relabeled.replace(r#"shutdown_after = "30m""#, r#"shutdown_after = "5m""#);
    let history = history_of(&[
        (at(18, 10, 0), "language = \"pt-BR\"\n"),
        (at(19, 23, 40), VALID),
        (at(19, 23, 50), relabeled.as_str()),
        (at(19, 23, 55), sooner.as_str()),
    ]);
    let since =
        |when: ZonedDateTime| rule_since(&history, when.timestamp(), Weekday::Thursday, "bedtime");
    assert_eq!(since(at(19, 23, 39)), None);
    assert_eq!(since(at(19, 23, 52)), Some(at(19, 23, 40).timestamp()));
    assert_eq!(since(at(20, 0, 0)), Some(at(19, 23, 40).timestamp()));
}

#[test]
fn waiting_lists_what_is_still_to_come() {
    let base = no_jitter();
    let more = base.replace(MORE_EXTENSIONS.0, MORE_EXTENSIONS.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 12, 0), more.as_str()),
    ]);
    let pending = waiting(&history, at(18, 13, 0).timestamp());
    assert!(
        pending
            .iter()
            .all(|timed| timed.change.id == "change-more-extensions"
                && timed.effective_at == at(19, 12, 0).timestamp())
    );
    let days: Vec<Weekday> = pending
        .iter()
        .filter_map(|timed| match timed.change.scope {
            Scope::Rule { day, .. } => Some(day),
            _ => None,
        })
        .collect();
    assert_eq!(
        days,
        [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Sunday,
        ]
    );
    assert_eq!(pending.len(), days.len());
    assert!(waiting(&history, at(19, 12, 0).timestamp()).is_empty());
}

#[test]
fn shown_merges_the_days_of_one_change() {
    let base = no_jitter();
    let more = base.replace(MORE_EXTENSIONS.0, MORE_EXTENSIONS.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 12, 0), more.as_str()),
    ]);
    let entries = shown(&waiting(&history, at(18, 13, 0).timestamp()));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "change-more-extensions");
    assert_eq!(
        entries[0].args[..2],
        [
            ("rule", "bedtime".to_owned()),
            ("days", "mon, tue, wed, thu, sun".to_owned()),
        ]
    );
    assert_eq!(entries[0].effective_at, at(19, 12, 0).timestamp());
}

#[test]
fn shown_lists_changes_by_the_time_they_take_effect() {
    let base = no_jitter();
    let quiet_and_renamed = base.replace(NO_LOCK.0, "").replace(
        "id = \"bedtime\"\n",
        "id = \"bedtime\"\nlabel = \"Sleep\"\n",
    );
    let history = history_of(&[(at(18, 8, 0), base.as_str())]);
    let changes = plan_save(
        &history,
        &parse(&quiet_and_renamed).unwrap(),
        at(18, 12, 0).timestamp(),
    );
    let entries: Vec<_> = shown(&changes)
        .iter()
        .map(|entry| (entry.id, entry.effective_at))
        .collect();
    assert_eq!(
        entries,
        [
            ("change-label", at(18, 12, 0).timestamp()),
            ("change-lock-removed", at(19, 12, 0).timestamp()),
        ]
    );
}

#[test]
fn defaulted_notices_follow_the_action_in_force() {
    let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();
    let base = no_jitter().replace("notices = [\"5m\", \"15m\", \"1m\"]\n", "");
    let quiet = base.replace(NO_LOCK.0, NO_LOCK.1);
    let history = history_of(&[
        (at(18, 8, 0), base.as_str()),
        (at(18, 12, 0), quiet.as_str()),
    ]);
    let now = rule(&config_at(&history, at(18, 12, 0).timestamp()));
    assert!(matches!(now.action, Action::Lock { .. }));
    assert_eq!(
        now.notice_offsets(),
        [15, 5, 1].map(SignedDuration::from_mins)
    );
    let later = rule(&config_at(&history, at(19, 12, 0).timestamp()));
    assert_eq!(later.action, Action::Nothing);
    assert_eq!(later.notice_offsets(), [SignedDuration::ZERO]);
}

fn with_steps(bodies: &[&str]) -> String {
    let tables: String = bodies
        .iter()
        .map(|body| format!("\n[[rule_set.rule.agent]]\n{body}\n"))
        .collect();
    format!("{}{tables}", no_jitter())
}

fn with_step(body: &str) -> String {
    with_steps(&[body])
}

fn thursday_steps(history: &History, when: ZonedDateTime) -> Vec<AgentStep> {
    config_at(history, when.timestamp()).rules_on(Weekday::Thursday)[0]
        .agent
        .clone()
}

fn thursday_step(history: &History, when: ZonedDateTime) -> AgentStep {
    thursday_steps(history, when)[0].clone()
}

fn levels(history: &History, when: ZonedDateTime) -> Vec<AgentLevel> {
    let mut levels: Vec<AgentLevel> = thursday_steps(history, when)
        .iter()
        .map(|step| step.level)
        .collect();
    levels.sort();
    levels
}

#[test]
fn a_new_level_applies_at_once_and_the_old_one_goes_after_the_delay() {
    use AgentLevel::{Light, SaveOnly};
    let light = with_step("before = \"60m\"\nlevel = \"light\"");
    let save_only = with_step("before = \"60m\"\nlevel = \"save-only\"");

    let history = history_of(&[(at(18, 8, 0), &save_only), (at(18, 12, 0), &light)]);
    assert_eq!(levels(&history, at(18, 12, 0)), [Light, SaveOnly]);
    assert_eq!(levels(&history, at(19, 11, 0)), [Light, SaveOnly]);
    assert_eq!(levels(&history, at(19, 12, 0)), [Light]);

    let history = history_of(&[(at(18, 8, 0), &light), (at(18, 12, 0), &save_only)]);
    assert_eq!(levels(&history, at(18, 12, 0)), [Light, SaveOnly]);
    assert_eq!(levels(&history, at(19, 12, 0)), [SaveOnly]);
}

#[test]
fn a_step_inserted_in_front_applies_at_once() {
    let before = with_step("before = \"60m\"\nlevel = \"save-only\"");
    let inserted = with_steps(&[
        "before = \"90m\"\nlevel = \"light\"",
        "before = \"60m\"\nlevel = \"save-only\"",
    ]);
    let history = history_of(&[(at(18, 8, 0), &before)]);
    let planned = plan_save(
        &history,
        &parse(&inserted).unwrap(),
        at(18, 12, 0).timestamp(),
    );
    assert!(!planned.is_empty());
    assert!(
        planned
            .iter()
            .all(|timed| timed.effective_at == at(18, 12, 0).timestamp())
    );
    let history = history_of(&[(at(18, 8, 0), &before), (at(18, 12, 0), &inserted)]);
    assert_eq!(
        levels(&history, at(18, 12, 0)),
        [AgentLevel::Light, AgentLevel::SaveOnly]
    );
}

#[test]
fn reordering_steps_plans_nothing() {
    let light = "before = \"60m\"\nlevel = \"light\"";
    let save_only = "before = \"30m\"\nlevel = \"save-only\"";
    let history = history_of(&[(at(18, 8, 0), &with_steps(&[light, save_only]))]);
    let reordered = with_steps(&[save_only, light]);
    let planned = plan_save(
        &history,
        &parse(&reordered).unwrap(),
        at(18, 12, 0).timestamp(),
    );
    assert!(planned.is_empty());
}

#[test]
fn a_firmer_step_applies_at_once_while_another_removed_waits() {
    let wide = "before = \"60m\"\nlevel = \"light\"";
    let narrow = "before = \"60m\"\nlevel = \"light\"\nfolders = [\"~/a\"]";
    let second = "before = \"30m\"\nlevel = \"save-only\"";
    let history = history_of(&[
        (at(18, 8, 0), &with_steps(&[narrow, second])),
        (at(18, 12, 0), &with_steps(&[wide])),
    ]);
    let folders = |when: ZonedDateTime| -> Vec<(AgentLevel, usize)> {
        let mut steps: Vec<(AgentLevel, usize)> = thursday_steps(&history, when)
            .iter()
            .map(|step| (step.level, step.folders.len()))
            .collect();
        steps.sort();
        steps
    };
    assert_eq!(
        folders(at(18, 12, 0)),
        [(AgentLevel::Light, 0), (AgentLevel::SaveOnly, 0)]
    );
    assert_eq!(
        folders(at(19, 11, 0)),
        [(AgentLevel::Light, 0), (AgentLevel::SaveOnly, 0)]
    );
    assert_eq!(folders(at(19, 12, 0)), [(AgentLevel::Light, 0)]);
}

#[test]
fn a_changed_before_adds_the_new_step_at_once_and_drops_the_old_one_later() {
    let history = history_of(&[
        (
            at(18, 8, 0),
            &with_step("before = \"60m\"\nlevel = \"light\""),
        ),
        (
            at(18, 12, 0),
            &with_step("before = \"30m\"\nlevel = \"light\""),
        ),
    ]);
    let befores = |when: ZonedDateTime| -> Vec<i64> {
        let mut befores: Vec<i64> = thursday_steps(&history, when)
            .iter()
            .map(|step| step.before.as_mins())
            .collect();
        befores.sort_unstable_by(|a, b| b.cmp(a));
        befores
    };
    assert_eq!(befores(at(18, 12, 0)), [60, 30]);
    assert_eq!(befores(at(19, 12, 0)), [30]);
}

#[test]
fn an_agent_message_edit_applies_at_once() {
    let first = with_step("before = \"60m\"\nlevel = \"light\"\nmessage = \"Wrap up\"");
    let second = with_step("before = \"60m\"\nlevel = \"light\"\nmessage = \"Time to stop\"");
    let saved_at = || at(18, 12, 0);
    let history = history_of(&[(at(18, 8, 0), &first)]);
    let planned = plan_save(&history, &parse(&second).unwrap(), saved_at().timestamp());
    assert!(!planned.is_empty());
    assert!(
        planned
            .iter()
            .all(|timed| timed.effective_at == saved_at().timestamp())
    );
    let history = history_of(&[(at(18, 8, 0), &first), (saved_at(), &second)]);
    assert_eq!(
        thursday_step(&history, saved_at()).message,
        ["Time to stop".to_owned()]
    );
}

#[test]
fn allowed_commands_follow_the_loosening_clock() {
    let listed =
        |commands: &str| format!("{}\n[agent]\nallowed_commands = {commands}\n", no_jitter());
    let fewer = listed(r#"["git status"]"#);
    let more = listed(
        r#"["git status", "git diff", "git add", "git commit", "git stash push", "git push"]"#,
    );
    let allowed = |history: &History, when: ZonedDateTime| {
        config_at(history, when.timestamp()).agent_allowed_commands
    };

    let history = history_of(&[(at(18, 8, 0), &no_jitter()), (at(18, 12, 0), &fewer)]);
    assert_eq!(allowed(&history, at(18, 12, 0)), ["git status".to_owned()]);

    let history = history_of(&[(at(18, 8, 0), &no_jitter()), (at(18, 12, 0), &more)]);
    assert_eq!(allowed(&history, at(19, 11, 0)).len(), 5);
    assert_eq!(allowed(&history, at(19, 12, 0)).len(), 6);
}
