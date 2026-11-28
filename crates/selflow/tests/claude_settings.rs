mod support;

use std::ffi::OsStr;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use selflow::claude::{
    DeliveryWarning, HookEdit, delivery_warnings, install, parse_claude_version, uninstall,
};
use selflow::cli::commands::claude_with;
use selflow::cli::{ClaudeAction, ClaudeArgs, CliError, error_code, error_text};
use selflow::os::claude_settings_path_from;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::Localizer;
use serde_json::{Value, json};
use support::ScriptedPrompt;

const SELFLOW: &str = "/usr/bin/selflow";
const MATCHER: &str = "Edit|Write|MultiEdit|NotebookEdit|Bash";
const EVENTS: [(&str, &str); 4] = [
    ("SessionStart", "session-start"),
    ("SessionEnd", "session-end"),
    ("UserPromptSubmit", "prompt"),
    ("PreToolUse", "tool"),
];

fn users_settings() -> Value {
    json!({
        "env": {"CLAUDE_CODE_NO_FLICKER": "1"},
        "hooks": {
            "UserPromptSubmit": [{
                "hooks": [
                    {"type": "command", "command": "~/.claude/hooks/remind.sh"},
                    {"type": "command", "command": "~/.claude/hooks/log.sh"},
                    {"type": "command", "command": "~/.claude/hooks/tag.sh", "timeout": 10}
                ]
            }],
            "PreToolUse": [
                {"matcher": "Bash", "hooks": [{"type": "command", "command": "~/.claude/hooks/guard.sh"}]},
                {"matcher": "Edit|Write", "hooks": [{"type": "command", "command": "~/.claude/hooks/format.sh"}]}
            ]
        },
        "statusLine": {"type": "command", "command": "~/.claude/statusline.sh"}
    })
}

fn command(program: &str, arg: &str) -> String {
    format!("{program} claude hook {arg}")
}

fn selflow_group(event: &str, arg: &str) -> Value {
    let hooks = json!([{"type": "command", "command": command(SELFLOW, arg), "timeout": 5}]);
    match event {
        "PreToolUse" => json!({"matcher": MATCHER, "hooks": hooks}),
        _ => json!({"hooks": hooks}),
    }
}

fn text(value: &Value) -> String {
    serde_json::to_string(value).unwrap()
}

fn added(program: &str) -> Vec<HookEdit> {
    EVENTS
        .iter()
        .map(|(event, arg)| HookEdit::Added {
            event,
            command: command(program, arg),
        })
        .collect()
}

fn installed_into_empty_settings() -> String {
    let mut settings = json!({});
    install(&mut settings, SELFLOW);
    format!("{}\n", serde_json::to_string_pretty(&settings).unwrap())
}

#[test]
fn install_adds_one_group_per_event_and_keeps_the_users_hooks() {
    let mut settings = users_settings();
    let edits = install(&mut settings, SELFLOW);
    let user = users_settings();
    let expected = json!({
        "env": user["env"],
        "hooks": {
            "UserPromptSubmit": [
                user["hooks"]["UserPromptSubmit"][0],
                selflow_group("UserPromptSubmit", "prompt")
            ],
            "PreToolUse": [
                user["hooks"]["PreToolUse"][0],
                user["hooks"]["PreToolUse"][1],
                selflow_group("PreToolUse", "tool")
            ],
            "SessionStart": [selflow_group("SessionStart", "session-start")],
            "SessionEnd": [selflow_group("SessionEnd", "session-end")]
        },
        "statusLine": user["statusLine"]
    });
    assert_eq!(text(&settings), text(&expected));
    assert_eq!(edits, added(SELFLOW));
}

#[test]
fn installing_twice_changes_nothing() {
    let mut settings = users_settings();
    install(&mut settings, SELFLOW);
    let once = text(&settings);
    assert_eq!(install(&mut settings, SELFLOW), []);
    assert_eq!(text(&settings), once);
}

#[test]
fn installing_from_another_path_replaces_the_old_entries() {
    let old = "/home/user/my tools/selflow";
    let quoted = "'/home/user/my tools/selflow'";
    let mut settings = users_settings();
    assert_eq!(install(&mut settings, old), added(quoted));
    let edits = install(&mut settings, SELFLOW);
    let expected: Vec<HookEdit> = EVENTS
        .iter()
        .flat_map(|(event, arg)| {
            [
                HookEdit::Removed {
                    event: (*event).to_owned(),
                    command: command(quoted, arg),
                },
                HookEdit::Added {
                    event,
                    command: command(SELFLOW, arg),
                },
            ]
        })
        .collect();
    assert_eq!(edits, expected);
    let mut fresh = users_settings();
    install(&mut fresh, SELFLOW);
    assert_eq!(text(&settings), text(&fresh));
}

#[test]
fn uninstall_removes_only_selflows_entries() {
    let mut settings = users_settings();
    install(&mut settings, SELFLOW);
    let edits = uninstall(&mut settings);
    let removed: Vec<HookEdit> = EVENTS
        .iter()
        .map(|(event, arg)| HookEdit::Removed {
            event: (*event).to_owned(),
            command: command(SELFLOW, arg),
        })
        .collect();
    assert_eq!(edits, removed);
    assert_eq!(text(&settings), text(&users_settings()));
    let mut empty = json!({});
    install(&mut empty, SELFLOW);
    uninstall(&mut empty);
    assert_eq!(empty, json!({}));
}

fn install_args(yes: bool) -> ClaudeArgs {
    ClaudeArgs {
        action: ClaudeAction::Install { yes },
    }
}

fn run(
    args: ClaudeArgs,
    path: &Path,
    version: Option<&str>,
    prompt: &mut ScriptedPrompt,
) -> Result<ExitCode, CliError> {
    let localizer = Localizer::new(Language::En);
    claude_with(args, path, SELFLOW, version, &localizer, prompt)
}

fn settings_file(contents: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("claude/settings.json");
    if let Some(contents) = contents {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
    }
    (dir, path)
}

#[test]
fn a_missing_file_gets_created() {
    let (_dir, path) = settings_file(None);
    let mut prompt = ScriptedPrompt::new([]);
    let code = run(
        install_args(true),
        &path,
        Some("2.1.100 (Claude Code)"),
        &mut prompt,
    );
    assert_eq!(code.unwrap(), ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        installed_into_empty_settings()
    );
    let shown = path.display();
    let mut said = vec![format!("Selflow will make these changes to {shown}:")];
    said.extend(
        EVENTS
            .iter()
            .map(|(event, arg)| format!("+ {event}: {}", command(SELFLOW, arg))),
    );
    said.push(
        "You have Claude Code 2.1.100. Reminders and blocked tool calls work, but messages to an \
         idle session need 2.1.224 or later."
            .to_owned(),
    );
    said.push(format!("Saved {shown}."));
    assert_eq!(prompt.said(), said);
}

#[test]
fn an_empty_file_counts_as_empty_settings() {
    let (_dir, path) = settings_file(Some(""));
    let mut prompt = ScriptedPrompt::new([]);
    let code = run(
        install_args(true),
        &path,
        Some("2.1.332 (Claude Code)"),
        &mut prompt,
    );
    assert_eq!(code.unwrap(), ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        installed_into_empty_settings()
    );
    let mut again = ScriptedPrompt::new([]);
    let code = run(
        install_args(true),
        &path,
        Some("2.1.332 (Claude Code)"),
        &mut again,
    );
    assert_eq!(code.unwrap(), ExitCode::SUCCESS);
    assert_eq!(
        again.said(),
        [format!("Nothing to change in {}.", path.display())]
    );
}

#[test]
fn invalid_json_is_left_untouched() {
    let original = "{\"hooks\": {";
    let (_dir, path) = settings_file(Some(original));
    let mut prompt = ScriptedPrompt::new([]);
    let error = run(install_args(true), &path, None, &mut prompt).unwrap_err();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(prompt.said().is_empty());
    assert_eq!(error_code(&error), 1);
    let message = error_text(&Localizer::new(Language::En), &error);
    let prefix = format!(
        "{} isn't valid JSON, so Selflow left it alone: ",
        path.display()
    );
    assert!(message.starts_with(&prefix), "{message}");
}

#[test]
fn declining_writes_nothing() {
    let original = text(&users_settings());
    let (_dir, path) = settings_file(Some(&original));
    fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
    let mut prompt = ScriptedPrompt::new(["n"]);
    let code = run(install_args(false), &path, None, &mut prompt);
    assert_eq!(code.unwrap(), ExitCode::from(1));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(
        prompt.said().last().unwrap(),
        &format!("Nothing was saved. {} is unchanged.", path.display())
    );
    let mut prompt = ScriptedPrompt::new([]);
    let code = run(install_args(true), &path, None, &mut prompt);
    assert_eq!(code.unwrap(), ExitCode::SUCCESS);
    let written: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let mut expected = users_settings();
    install(&mut expected, SELFLOW);
    assert_eq!(text(&written), text(&expected));
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640);
}

#[test]
fn delivery_warnings_cover_held_and_bypassed_inbound() {
    let held = |value: &str| DeliveryWarning::Held {
        value: value.to_owned(),
    };
    let bypass = json!({"defaultMode": "bypassPermissions"});
    let cases = [
        (json!({"crossSessionInbound": "hold"}), vec![held("hold")]),
        (
            json!({"crossSessionInbound": "refuse"}),
            vec![held("refuse")],
        ),
        (json!({"crossSessionInbound": "accept"}), vec![]),
        (
            json!({"permissions": bypass}),
            vec![DeliveryWarning::Bypassed],
        ),
        (
            json!({"permissions": bypass, "crossSessionInbound": "accept"}),
            vec![],
        ),
        (json!({"permissions": {"defaultMode": "auto"}}), vec![]),
    ];
    for (settings, warnings) in cases {
        assert_eq!(delivery_warnings(&settings), warnings, "{settings}");
    }
}

#[test]
fn the_claude_version_is_read() {
    assert_eq!(
        parse_claude_version("2.1.332 (Claude Code)"),
        Some((2, 1, 332))
    );
    assert_eq!(parse_claude_version("garbage"), None);
}

#[test]
fn the_settings_path_follows_claude_config_dir() {
    let path = |config: Option<&str>| {
        claude_settings_path_from(config.map(OsStr::new), Some(OsStr::new("/h")))
    };
    assert_eq!(path(Some("/c")), Some(PathBuf::from("/c/settings.json")));
    assert_eq!(path(None), Some(PathBuf::from("/h/.claude/settings.json")));
    assert_eq!(
        path(Some("")),
        Some(PathBuf::from("/h/.claude/settings.json"))
    );
}
