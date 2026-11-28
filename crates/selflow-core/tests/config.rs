mod support;

use jiff::{
    SignedDuration,
    civil::{Weekday, date, time},
};
use selflow_core::config::{
    Action, AgentLevel, AgentStep, Challenges, Config, Exception, Rule, RuleEnd, parse,
};
use selflow_i18n::{catalog::default_config, language::Language};
use support::{VALID, no_jitter};

fn mins(minutes: i64) -> SignedDuration {
    SignedDuration::from_mins(minutes)
}

fn thursday_rules(text: &str) -> Vec<Rule> {
    parse(text).unwrap().rules_on(Weekday::Thursday).to_vec()
}

fn first_rule(text: &str) -> Rule {
    thursday_rules(text).remove(0)
}

fn edited(edits: &[(&str, &str)]) -> String {
    edits.iter().fold(VALID.to_owned(), |text, (from, to)| {
        assert!(text.contains(from), "the fixture has no {from}");
        text.replace(from, to)
    })
}

fn error_ids(text: &str) -> Vec<&'static str> {
    parse(text).unwrap_err().iter().map(|e| e.id).collect()
}

const NO_LOCK: (&str, &str) = ("lock = true\nshutdown_after = \"30m\"\n", "");

const STEPS: &str = "[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\n\n[[rule_set.rule.agent]]\nbefore = \"15m\"\nlevel = \"save-only\"\nmessage = [\"a\", \"b\"]\nfolders = [\"~/Workspace/work\"]\nunattended = true\n";

fn with_steps(text: &str) -> String {
    format!("{text}\n{STEPS}")
}

fn light_step() -> AgentStep {
    AgentStep {
        before: mins(60),
        level: AgentLevel::Light,
        message: Vec::new(),
        folders: Vec::new(),
        unattended: false,
    }
}

const NO_NOTICES: (&str, &str) = ("notices = [\"5m\", \"15m\", \"1m\"]\n", "");

#[test]
fn parses_a_rule_with_a_lock_and_a_shutdown() {
    let config = parse(VALID).unwrap();
    let rule = &config.rules_on(Weekday::Thursday)[0];
    assert_eq!(
        (rule.id.as_str(), rule.label.as_str()),
        ("bedtime", "bedtime")
    );
    assert_eq!(
        (rule.start, rule.start_jitter, rule.end),
        (time(23, 0, 0, 0), mins(15), RuleEnd::At(time(7, 0, 0, 0)))
    );
    assert_eq!(rule.notices, Some(vec![mins(15), mins(5), mins(1)]));
    assert_eq!(rule.notice_offsets(), [mins(15), mins(5), mins(1)]);
    assert!(rule.message.is_empty());
    assert_eq!(
        rule.action,
        Action::Lock {
            shutdown_after: Some(mins(30))
        }
    );
    assert!(rule.enforces());
    assert_eq!(
        rule.challenges,
        Challenges {
            wait: mins(2),
            reason: true,
            phrase: None,
            extensions: 1,
            extension_length: mins(20),
        }
    );
    assert!(config.rules_on(Weekday::Friday).is_empty());
    assert!(config.rule_set_for(Weekday::Friday).is_none());
    assert!(config.exceptions.is_empty());
}

#[test]
fn a_rule_overrides_its_sets_challenges() {
    let text = format!(
        "{}\n[rule_set.rule.challenges]\nextensions = 0\nphrase = \"Rest is enough\"\n\n[[rule_set.rule]]\nid = \"evening\"\nstart = \"21:00\"\n",
        VALID.replace(r#"wait = "2m""#, r#"wait = "5m""#)
    );
    let rules = thursday_rules(&text);
    assert_eq!(
        rules[0].challenges,
        Challenges {
            wait: mins(5),
            reason: true,
            phrase: Some("Rest is enough".to_owned()),
            extensions: 0,
            extension_length: mins(20),
        }
    );
    assert_eq!(
        rules[1].challenges,
        Challenges {
            wait: mins(5),
            reason: true,
            phrase: None,
            extensions: 1,
            extension_length: mins(20),
        }
    );
}

#[test]
fn notices_default_by_action() {
    let without_notices = ("notices = [\"5m\", \"15m\", \"1m\"]\n", "");
    let lock = first_rule(&edited(&[without_notices]));
    assert_eq!(lock.notices, None);
    assert_eq!(lock.notice_offsets(), [mins(15), mins(5), mins(1)]);
    let shutdown = first_rule(&edited(&[
        without_notices,
        (NO_LOCK.0, "shutdown = \"once\"\n"),
    ]));
    assert_eq!(shutdown.notices, None);
    assert_eq!(shutdown.notice_offsets(), [mins(15), mins(5), mins(1)]);
    let reminder = first_rule(&edited(&[without_notices, NO_LOCK]));
    assert_eq!(reminder.action, Action::Nothing);
    assert!(!reminder.enforces());
    assert_eq!(reminder.notices, None);
    assert_eq!(reminder.notice_offsets(), [SignedDuration::ZERO]);
    let steps_only = first_rule(&with_steps(&edited(&[NO_NOTICES, NO_LOCK])));
    assert_eq!(steps_only.notices, None);
    assert!(steps_only.notice_offsets().is_empty());
    let with_notice = first_rule(&with_steps(&edited(&[
        ("notices = [\"5m\", \"15m\", \"1m\"]", "notices = [\"5m\"]"),
        NO_LOCK,
    ])));
    assert_eq!(with_notice.notice_offsets(), [mins(5)]);
}

#[test]
fn parses_agent_steps() {
    let rule = first_rule(&with_steps(VALID));
    assert_eq!(
        rule.agent,
        vec![
            light_step(),
            AgentStep {
                before: mins(15),
                level: AgentLevel::SaveOnly,
                message: vec!["a".to_owned(), "b".to_owned()],
                folders: vec!["~/Workspace/work".to_owned()],
                unattended: true,
            },
        ]
    );
    assert!(first_rule(VALID).agent.is_empty());
    let steps_only = first_rule(&with_steps(&edited(&[NO_LOCK])));
    assert_eq!(steps_only.action, Action::Nothing);
    assert!(steps_only.enforces());
}

#[test]
fn an_agent_step_can_be_a_single_message_text_and_absolute_folders() {
    let text = format!(
        "{VALID}\n[[rule_set.rule.agent]]\nbefore = \"0m\"\nlevel = \"message\"\nmessage = \"Wrap up\"\nfolders = [\"~\", \"/srv/work\"]\n"
    );
    let step = first_rule(&text).agent.remove(0);
    assert_eq!(
        step,
        AgentStep {
            before: SignedDuration::ZERO,
            level: AgentLevel::Message,
            message: vec!["Wrap up".to_owned()],
            folders: vec!["~".to_owned(), "/srv/work".to_owned()],
            unattended: false,
        }
    );
}

#[test]
fn the_levels_run_from_lightest_to_firmest_and_have_names() {
    let levels = [
        AgentLevel::Message,
        AgentLevel::Light,
        AgentLevel::NoNewWork,
        AgentLevel::SaveOnly,
    ];
    assert!(levels.is_sorted());
    let names: Vec<_> = levels.iter().map(|level| level.name()).collect();
    assert_eq!(names, ["message", "light", "no-new-work", "save-only"]);
}

#[test]
fn the_allowed_commands_default_to_the_git_save_commands() {
    assert_eq!(
        parse(VALID).unwrap().agent_allowed_commands,
        [
            "git status",
            "git diff",
            "git add",
            "git commit",
            "git stash push"
        ]
    );
}

#[test]
fn the_allowed_commands_come_from_the_agent_table_in_order() {
    let text = format!("{VALID}\n[agent]\nallowed_commands = [\"cargo fmt\", \"git status\"]\n");
    assert_eq!(
        parse(&text).unwrap().agent_allowed_commands,
        ["cargo fmt", "git status"]
    );
    let none = format!("{VALID}\n[agent]\nallowed_commands = []\n");
    assert!(parse(&none).unwrap().agent_allowed_commands.is_empty());
}

#[test]
fn a_rule_can_shut_down_without_a_lock() {
    let shutting_down = |how: &str| {
        first_rule(&edited(&[
            ("end = \"wake\"\n", ""),
            (NO_LOCK.0, &format!("shutdown = \"{how}\"\n")),
        ]))
    };
    let once = shutting_down("once");
    assert_eq!(once.action, Action::ShutDown { repeat: false });
    assert_eq!(once.end, RuleEnd::DayEnd);
    assert!(once.enforces());
    assert_eq!(
        shutting_down("repeat").action,
        Action::ShutDown { repeat: true }
    );
}

#[test]
fn a_message_is_one_text_or_a_list() {
    let with_message = |message: &str| {
        first_rule(&edited(&[(
            "lock = true",
            &format!("message = {message}\nlock = true"),
        )]))
        .message
    };
    assert_eq!(with_message(r#""Time to rest""#), ["Time to rest"]);
    assert_eq!(
        with_message(r#"["Time to rest", "The day is done"]"#),
        ["Time to rest", "The day is done"]
    );
    assert!(first_rule(VALID).message.is_empty());
}

#[test]
fn a_rule_can_last_a_length_instead_of_ending_at_a_time() {
    let rule = first_rule(&edited(&[(r#"end = "wake""#, r#"length = "8h""#)]));
    assert_eq!(rule.end, RuleEnd::After(SignedDuration::from_hours(8)));
    let longest = first_rule(&no_jitter().replace(r#"end = "wake""#, r#"length = "23h""#));
    assert_eq!(longest.end, RuleEnd::After(SignedDuration::from_hours(23)));
}

#[test]
fn an_exception_names_a_date_and_maybe_rules() {
    let text = format!(
        "{VALID}\n[[exception]]\ndate = \"2026-11-19\"\nrules = [\"bedtime\"]\n\n[[exception]]\ndate = \"2026-11-24\"\n"
    );
    let config = parse(&text).unwrap();
    assert_eq!(
        config.exceptions,
        vec![
            Exception {
                date: date(2026, 11, 19),
                rules: vec!["bedtime".to_owned()],
            },
            Exception {
                date: date(2026, 11, 24),
                rules: Vec::new(),
            },
        ]
    );
    let named = config.exception_on(date(2026, 11, 19)).unwrap();
    assert!(named.covers("bedtime"));
    assert!(!named.covers("evening"));
    assert!(
        config
            .exception_on(date(2026, 11, 24))
            .unwrap()
            .covers("bedtime")
    );
    assert_eq!(config.exception_on(date(2026, 11, 20)), None);
}

#[test]
fn reports_each_error_with_its_line() {
    let cases: &[(&str, &str, &str, usize)] = &[
        (
            r#"wait = "2m""#,
            r#"wait = "soon""#,
            "config-bad-duration",
            15,
        ),
        (
            r#"start = "bedtime""#,
            r#"start = "bedtme""#,
            "config-unknown-anchor",
            22,
        ),
        (
            r#"notices = ["5m", "15m", "1m"]"#,
            r#"notices = ["5m", "-15m", "1m"]"#,
            "config-negative-offset",
            25,
        ),
        (
            r#"end = "wake""#,
            "end = \"wake\"\nlength = \"8h\"",
            "config-end-and-length",
            25,
        ),
        ("end = \"wake\"\n", "", "config-lock-needs-end", 25),
        ("lock = true\n", "", "config-shutdown-after-needs-lock", 26),
        (
            r#"shutdown_after = "30m""#,
            r#"shutdown = "once""#,
            "config-shutdown-with-lock",
            27,
        ),
        (
            "lock = true\nshutdown_after = \"30m\"",
            r#"shutdown = "twice""#,
            "config-bad-shutdown",
            26,
        ),
        (
            "lock = true",
            "message = 3\nlock = true",
            "config-bad-message",
            26,
        ),
        (
            "lock = true",
            "message = [\"Rest\", \"\"]\nlock = true",
            "config-empty-message",
            26,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[exception]]\ndate = \"19/11/2026\"\n",
            "config-bad-date",
            30,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[exception]]\ndate = \"2026-11-19\"\n\n[[exception]]\ndate = \"2026-11-19\"\n",
            "config-duplicate-exception",
            33,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[exception]]\ndate = \"2026-11-19\"\nrules = [\"nap\"]\n",
            "config-exception-unknown-rule",
            31,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule]]\nid = \"bedtime\"\nstart = \"12:00\"\n",
            "config-duplicate-rule",
            30,
        ),
        (
            r#"wake = "07:00""#,
            r#"wake = "23:15""#,
            "config-jitter-too-long",
            23,
        ),
        (
            "start = \"bedtime\"\nstart_jitter = \"15m\"\nend = \"wake\"\nnotices = [\"5m\", \"15m\", \"1m\"]\nlock = true\nshutdown_after = \"30m\"",
            "start = \"04:50\"\nstart_jitter = \"15m\"",
            "config-jitter-too-long",
            23,
        ),
        (
            r#"end = "wake""#,
            r#"length = "80h""#,
            "config-too-long",
            24,
        ),
        (
            r#"end = "wake""#,
            r#"length = "23h50m""#,
            "config-too-long",
            24,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"loud\"\n",
            "config-agent-bad-level",
            31,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\nfolders = [\"~/ok\", \"Workspace/x\"]\n",
            "config-agent-bad-folder",
            32,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\nfolders = [\"~/Workspace/seconv/../other\"]\n",
            "config-agent-bad-folder",
            32,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\nfolders = [\"/srv/./code\"]\n",
            "config-agent-bad-folder",
            32,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\n\n[[rule_set.rule.agent]]\nbefore = \"1h\"\nlevel = \"light\"\nmessage = \"Wind down.\"\n",
            "config-agent-duplicate-step",
            34,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"24h\"\nlevel = \"light\"\n",
            "config-agent-too-early",
            30,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[agent]\nallowed_commands = [\" \"]\n",
            "config-agent-empty-command",
            30,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"-5m\"\nlevel = \"light\"\n",
            "config-negative-offset",
            30,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\nmessage = 3\n",
            "config-bad-message",
            32,
        ),
        (
            "shutdown_after = \"30m\"\n",
            "shutdown_after = \"30m\"\n\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\nmessage = [\"\"]\n",
            "config-empty-message",
            32,
        ),
    ];
    for &(from, to, id, line) in cases {
        let found: Vec<_> = parse(&edited(&[(from, to)]))
            .unwrap_err()
            .iter()
            .map(|e| (e.id, e.line))
            .collect();
        assert_eq!(found, vec![(id, line)], "{id}");
    }
}

#[test]
fn an_unknown_rule_in_an_exception_names_the_date_and_the_rule() {
    let text = format!("{VALID}\n[[exception]]\ndate = \"2026-11-20\"\nrules = [\"bedtime\"]\n");
    let errors = parse(&text).unwrap_err();
    assert_eq!(errors[0].id, "config-exception-unknown-rule");
    assert_eq!(
        errors[0].args,
        vec![("date", "2026-11-20".into()), ("rule", "bedtime".into())]
    );
}

#[test]
fn rejects_unknown_keys_and_the_m1_lock_table() {
    let text = VALID.replace(
        "language = \"pt-BR\"",
        "language = \"pt-BR\"\ncolour = \"red\"",
    );
    let errors = parse(&text).unwrap_err();
    assert_eq!((errors[0].id, errors[0].line), ("config-syntax", 3));
    let m1 = VALID.replace("[[rule_set.rule]]", "[[rule_set.lock]]");
    let errors = parse(&m1).unwrap_err();
    assert_eq!((errors[0].id, errors[0].line), ("config-syntax", 20));
    let step = format!(
        "{VALID}\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\ncolour = \"red\"\n"
    );
    let errors = parse(&step).unwrap_err();
    assert_eq!((errors[0].id, errors[0].line), ("config-syntax", 32));
}

#[test]
fn rejects_a_day_claimed_by_two_rule_sets() {
    let text = format!("{VALID}\n[[rule_set]]\nname = \"weekend\"\ndays = [\"mon\", \"fri\"]\n");
    let errors = parse(&text).unwrap_err();
    assert_eq!(errors[0].id, "config-day-in-two-sets");
    assert_eq!(
        errors[0].args,
        vec![
            ("day", "mon".into()),
            ("first", "weeknight".into()),
            ("second", "weekend".into())
        ]
    );
}

fn with_rule(text: &str, rule: &str) -> String {
    format!("{text}\n[[rule_set.rule]]\n{rule}\n")
}

const LATE: &str = "id = \"late\"\nstart = \"01:00\"\nend = \"06:00\"";
const MORNING: &str = "id = \"morning\"\nstart = \"06:00\"\nend = \"08:00\"";

#[test]
fn rejects_locks_that_overlap_in_one_rule_set() {
    let late_lock = format!("{LATE}\nlock = true");
    assert_eq!(
        error_ids(&with_rule(VALID, &late_lock)),
        ["config-locks-overlap"]
    );
    assert!(parse(&with_rule(VALID, LATE)).is_ok());
}

#[test]
fn rejects_a_lock_that_overlaps_the_night_lock_after_the_day_start() {
    let errors = parse(&with_rule(VALID, &format!("{MORNING}\nlock = true"))).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].id, "config-locks-overlap");
    assert_eq!(
        errors[0].args,
        vec![
            ("first", "bedtime".into()),
            ("second", "morning".into()),
            ("set", "weeknight".into())
        ]
    );
}

#[test]
fn compares_locks_across_the_day_start_only_on_consecutive_days() {
    let text = VALID.replace(
        r#"days = ["sun", "mon", "tue", "wed", "thu"]"#,
        r#"days = ["mon", "wed"]"#,
    );
    assert!(parse(&with_rule(&text, &format!("{MORNING}\nlock = true"))).is_ok());
}

#[test]
fn reports_an_empty_window_next_to_a_bad_jitter() {
    let text = edited(&[
        (r#"start_jitter = "15m""#, r#"start_jitter = "soon""#),
        (r#"end = "wake""#, r#"end = "bedtime""#),
    ]);
    assert_eq!(
        error_ids(&text),
        ["config-bad-duration", "config-empty-window"]
    );
}

#[test]
fn reports_an_empty_window_when_the_day_start_is_bad() {
    let text = edited(&[
        (r#"day_starts_at = "05:00""#, r#"day_starts_at = "5am""#),
        (r#"end = "wake""#, r#"end = "bedtime""#),
    ]);
    assert_eq!(error_ids(&text), ["config-bad-time", "config-empty-window"]);
}

#[test]
fn the_default_files_parse_in_both_languages() {
    let without_labels = |mut config: Config| {
        config
            .rule_sets
            .iter_mut()
            .flat_map(|set| &mut set.rules)
            .for_each(|rule| rule.label.clear());
        (config.rule_sets, config.exceptions)
    };
    let en = parse(default_config(Language::En)).unwrap();
    let pt = parse(default_config(Language::PtBr)).unwrap();
    assert_eq!(
        (en.language.as_str(), pt.language.as_str()),
        ("en", "pt-BR")
    );
    let set = &en.rule_sets[0];
    assert_eq!((set.name.as_str(), set.days.len()), ("every-night", 7));
    let bedtime = &set.rules[0];
    assert_eq!(
        (bedtime.id.as_str(), bedtime.label.as_str()),
        ("bedtime", "Bedtime")
    );
    assert_eq!(pt.rule_sets[0].rules[0].label, "Hora de dormir");
    assert_eq!(
        (bedtime.start, bedtime.start_jitter, bedtime.end),
        (time(23, 0, 0, 0), mins(15), RuleEnd::At(time(7, 0, 0, 0)))
    );
    assert_eq!(bedtime.notices, Some(vec![mins(15), mins(5), mins(1)]));
    assert_eq!(
        bedtime.action,
        Action::Lock {
            shutdown_after: Some(mins(30))
        }
    );
    let wrap_up = AgentStep {
        before: mins(15),
        level: AgentLevel::NoNewWork,
        ..light_step()
    };
    assert_eq!(bedtime.agent, vec![light_step(), wrap_up]);
    assert_eq!(pt.rule_sets[0].rules[0].agent, bedtime.agent);
    assert_eq!(
        en.agent_allowed_commands,
        [
            "git status",
            "git diff",
            "git add",
            "git commit",
            "git stash push"
        ]
    );
    assert!(en.exceptions.is_empty());
    assert_eq!(without_labels(en), without_labels(pt));
}
