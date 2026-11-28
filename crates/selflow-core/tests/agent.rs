mod support;

use std::path::Path;

use jiff::{SignedDuration, ZonedDateTime, civil::date};
use selflow_core::agent::{ActiveAgentStep, command_allowed, covers, folders_cover, governing};
use selflow_core::config::{AgentLevel, AgentStep, Config, parse};
use selflow_core::enforce::{DayLog, Evaluation, evaluate};
use selflow_core::plan::windows_for;
use selflow_core::timing::Seed;
use support::{VALID, at, brt, no_jitter};

const LIGHT_STEP: &str = "\n[[rule_set.rule.agent]]\nbefore = \"60m\"\nlevel = \"light\"\n";

fn with_light_step(text: &str) -> Config {
    parse(&format!("{text}{LIGHT_STEP}")).unwrap()
}

fn evaluated(now: &ZonedDateTime, config: &Config) -> Evaluation {
    evaluate(now, config, Seed(7), &DayLog::default())
}

fn active_keys(now: &ZonedDateTime, config: &Config) -> Vec<String> {
    evaluated(now, config)
        .agent
        .into_iter()
        .map(|active| active.key)
        .collect()
}

fn step(level: AgentLevel, folders: &[&str], unattended: bool) -> AgentStep {
    AgentStep {
        before: SignedDuration::from_mins(60),
        level,
        message: Vec::new(),
        folders: folders.iter().map(|folder| (*folder).to_owned()).collect(),
        unattended,
    }
}

fn active(step: AgentStep, minute: i8) -> ActiveAgentStep {
    let window = windows_for(
        &parse(&no_jitter()).unwrap(),
        Seed(7),
        date(2026, 11, 19),
        &brt(),
    )
    .remove(0);
    ActiveAgentStep {
        key: format!("bedtime/2026-11-19/{minute}m/{}", step.level.name()),
        window,
        step,
        at: at(19, 22, minute).timestamp(),
        message: None,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn home() -> Option<&'static Path> {
    Some(Path::new("/home/u"))
}

#[test]
fn a_step_is_active_from_its_time_until_the_rule_ends() {
    let c = with_light_step(&no_jitter());
    assert!(active_keys(&at(19, 21, 59), &c).is_empty());
    let started = evaluated(&at(19, 22, 0), &c).agent;
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].key, "bedtime/2026-11-19/60m/light");
    assert_eq!(started[0].at, at(19, 22, 0).timestamp());
    assert_eq!(started[0].step.level, AgentLevel::Light);
    assert_eq!(started[0].window.ends_at, at(20, 7, 0));
    assert_eq!(started[0].message, None);
    assert_eq!(
        active_keys(&at(20, 6, 59), &c),
        ["bedtime/2026-11-19/60m/light"]
    );
    assert!(active_keys(&at(20, 7, 0), &c).is_empty());
}

#[test]
fn the_engine_wakes_for_the_next_step() {
    let c = with_light_step(&no_jitter());
    assert_eq!(
        evaluated(&at(19, 21, 30), &c).next_change_at,
        Some(at(19, 22, 0).timestamp())
    );
}

#[test]
fn a_step_moves_with_the_start_jitter() {
    let c = with_light_step(VALID);
    let window = windows_for(&c, Seed(7), date(2026, 11, 19), &brt()).remove(0);
    let agent = evaluated(&at(19, 22, 30), &c).agent;
    assert_eq!(agent.len(), 1);
    assert_eq!(
        agent[0].at,
        window
            .starts_at
            .timestamp()
            .checked_sub(SignedDuration::from_mins(60))
            .unwrap()
    );
}

#[test]
fn a_step_message_is_one_of_its_texts_for_the_whole_day() {
    let c = parse(&format!(
        "{}{LIGHT_STEP}message = [\"Wind down.\", \"Start wrapping up.\"]\n",
        no_jitter()
    ))
    .unwrap();
    let message_at = |now: ZonedDateTime| evaluated(&now, &c).agent[0].message.clone().unwrap();
    let chosen = message_at(at(19, 22, 0));
    assert!(["Wind down.", "Start wrapping up."].contains(&chosen.as_str()));
    assert_eq!(message_at(at(20, 3, 0)), chosen);
}

#[test]
fn an_exception_day_drops_the_steps() {
    let c = parse(&format!(
        "{}{LIGHT_STEP}\n[[exception]]\ndate = \"2026-11-19\"\n",
        no_jitter()
    ))
    .unwrap();
    assert!(evaluated(&at(19, 22, 30), &c).agent.is_empty());
}

#[test]
fn a_started_level_holds_after_its_loosening_applies() {
    let log = DayLog {
        started: windows_for(
            &with_light_step(&no_jitter()),
            Seed(7),
            date(2026, 11, 19),
            &brt(),
        ),
        ..DayLog::default()
    };
    let without_step = parse(&no_jitter()).unwrap();
    let agent = evaluate(&at(19, 23, 30), &without_step, Seed(7), &log).agent;
    assert_eq!(
        agent
            .iter()
            .map(|active| active.key.as_str())
            .collect::<Vec<_>>(),
        ["bedtime/2026-11-19/60m/light"]
    );
    let reworded = parse(&format!(
        "{}{LIGHT_STEP}message = \"Wind down.\"\n",
        no_jitter()
    ))
    .unwrap();
    let agent = evaluate(&at(19, 23, 30), &reworded, Seed(7), &log).agent;
    assert_eq!(agent.len(), 1);
    assert_eq!(agent[0].step, log.started[0].agent[0]);
}

#[test]
fn a_rule_with_only_steps_sends_no_notice() {
    let steps_only = "[[rule_set]]\nname = \"weekdays\"\ndays = [\"thu\"]\n\n[[rule_set.rule]]\nid = \"end-of-work\"\nstart = \"18:00\"\n";
    let quiet = with_light_step(steps_only);
    let before = evaluated(&at(19, 16, 0), &quiet);
    assert!(before.notices.is_empty());
    assert!(before.agent.is_empty());
    assert_eq!(
        active_keys(&at(19, 17, 0), &quiet),
        ["end-of-work/2026-11-19/60m/light"]
    );
    let at_start = with_light_step(&format!("{steps_only}notices = [\"0m\"]\n"));
    assert_eq!(
        evaluated(&at(19, 16, 0), &at_start)
            .notices
            .iter()
            .map(|notice| notice.at)
            .collect::<Vec<_>>(),
        [at(19, 18, 0).timestamp()]
    );
}

#[test]
fn a_folder_covers_what_is_inside_it() {
    let seconv = step(AgentLevel::Light, &["~/Workspace/seconv"], false);
    assert!(covers(
        &seconv,
        Path::new("/home/u/Workspace/seconv"),
        home()
    ));
    assert!(covers(
        &seconv,
        Path::new("/home/u/Workspace/seconv/src"),
        home()
    ));
    assert!(!covers(
        &seconv,
        Path::new("/home/u/Workspace/seconv-old"),
        home()
    ));
    assert!(!covers(&seconv, Path::new("/home/u/Workspace"), home()));
    let whole_home = step(AgentLevel::Light, &["~"], false);
    assert!(covers(&whole_home, Path::new("/home/u"), home()));
    assert!(covers(&whole_home, Path::new("/home/u/x"), home()));
    assert!(!covers(&whole_home, Path::new("/home/user2/x"), home()));
    assert!(!covers(&whole_home, Path::new("/tmp/x"), home()));
    let everywhere = step(AgentLevel::Light, &[], false);
    assert!(covers(&everywhere, Path::new("/tmp/x"), home()));
    assert!(covers(&everywhere, Path::new("/tmp/x"), None));
    let code = step(AgentLevel::Light, &["/srv/code"], false);
    assert!(covers(&code, Path::new("/srv/code/a"), home()));
    assert!(covers(&code, Path::new("/srv/code/a"), None));
    assert!(!covers(&code, Path::new("/srv/codex"), home()));
}

#[test]
fn without_a_home_only_absolute_folders_cover() {
    let both = step(AgentLevel::Light, &["~", "/srv/code"], false);
    assert!(!covers(&both, Path::new("/home/u/x"), None));
    assert!(!covers(&both, Path::new("~/x"), None));
    assert!(covers(&both, Path::new("/srv/code/a"), None));
}

#[test]
fn the_firmest_level_governs() {
    let steps = [
        active(step(AgentLevel::Light, &[], false), 0),
        active(step(AgentLevel::SaveOnly, &["~/Workspace/work"], false), 30),
    ];
    let level_at =
        |cwd: &str| governing(&steps, Path::new(cwd), home(), true).map(|active| active.step.level);
    assert_eq!(
        level_at("/home/u/Workspace/work/a"),
        Some(AgentLevel::SaveOnly)
    );
    assert_eq!(level_at("/home/u/notes"), Some(AgentLevel::Light));
    let tied = [
        active(step(AgentLevel::Light, &[], false), 45),
        active(step(AgentLevel::Light, &[], false), 15),
    ];
    assert_eq!(
        governing(&tied, Path::new("/home/u/notes"), home(), true).map(|active| active.at),
        Some(at(19, 22, 45).timestamp())
    );
    let narrow = [active(
        step(AgentLevel::Light, &["~/Workspace/work"], false),
        0,
    )];
    assert_eq!(
        governing(&narrow, Path::new("/home/u/notes"), home(), true),
        None
    );
}

#[test]
fn unattended_runs_need_a_step_that_says_so() {
    let watched = [active(step(AgentLevel::SaveOnly, &[], false), 0)];
    assert_eq!(
        governing(&watched, Path::new("/home/u/x"), home(), false),
        None
    );
    assert!(governing(&watched, Path::new("/home/u/x"), home(), true).is_some());
    let scripted = [active(step(AgentLevel::SaveOnly, &[], true), 0)];
    assert_eq!(
        governing(&scripted, Path::new("/home/u/x"), home(), false),
        Some(&scripted[0])
    );
}

#[test]
fn only_plain_allowed_commands_pass() {
    let allowed = Config::empty().agent_allowed_commands;
    for command in [
        "git status",
        "  git diff --stat",
        "git commit -m \"save work\"",
        "git commit -m \"x\"",
        "git add -A",
        "git stash push -m wip",
        "git stash\tpush",
    ] {
        assert!(command_allowed(command, &allowed), "{command:?}");
    }
    for command in [
        "git push",
        "git",
        "",
        "gitstatus",
        "git statusx",
        "git\u{a0}status",
        "GIT_DIR=x git status",
        "git stash",
        "git stash pop",
        "git stash apply",
        "git stash drop",
        "git commit -m x && rm -rf src",
        "git status || rm f",
        "git status & rm f",
        "git status; touch f",
        "git add $(cat list)",
        "git diff > out.txt",
        "git diff < in.txt",
        "git status | sh",
        "echo `id`",
        "git stash push\nrm f",
        "git add (rm f)",
        "git diff <(rm f)",
        "git commit -m $HOME",
        r"git status ${x:=$'\x24\x28touch PWNED_P\x29'} ${x@P}",
        r"git status ${y:=$'a[\x24\x28touch PWNED_ARITH\x29]'} ${HOME:y}",
        r"git status ${y:=$'a[\x24\x28touch PWNED_Z3\x29]'} $[y]",
        "git diff {--output=src/main.rs,}",
    ] {
        assert!(!command_allowed(command, &allowed), "{command:?}");
    }
}

#[test]
fn arguments_that_make_git_write_files_are_denied() {
    let allowed = Config::empty().agent_allowed_commands;
    for command in [
        "git diff --output=f",
        "git diff --output f",
        "git diff --output-indicator-new=x",
        "git diff --stat --output=src/main.rs",
        "git diff \"--output=f\"",
        "git diff '--output'=f",
        "git diff --o\"\"utput=f",
        r"git diff --outp\ut=f",
    ] {
        assert!(!command_allowed(command, &allowed), "{command:?}");
    }
}

#[test]
fn an_allowed_entry_with_no_words_allows_nothing() {
    assert!(!command_allowed("rm -rf src", &strings(&["  "])));
    assert!(!command_allowed("git status", &[]));
    assert!(command_allowed(
        "cargo fmt --all",
        &strings(&["cargo  fmt"])
    ));
}

#[test]
fn folders_cover_compares_components() {
    assert!(folders_cover(&strings(&["~/a"]), &strings(&["~/a/b"])));
    assert!(folders_cover(&strings(&["~/a"]), &strings(&["~/a"])));
    assert!(!folders_cover(&strings(&["~/a"]), &strings(&["~/ab"])));
    assert!(folders_cover(
        &strings(&["~/a", "/srv"]),
        &strings(&["/srv/x", "~/a/b"])
    ));
    assert!(!folders_cover(
        &strings(&["~/a"]),
        &strings(&["~/a/b", "/srv/x"])
    ));
    assert!(!folders_cover(&strings(&["~"]), &strings(&["/srv/x"])));
    assert!(folders_cover(&[], &strings(&["~/x"])));
    assert!(folders_cover(&[], &[]));
    assert!(!folders_cover(&strings(&["~"]), &[]));
}
