mod support;

use std::ffi::OsString;
use std::io::{Cursor, Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use selflow::claude::{HookEnv, attended, hook_event, hook_output, hook_request, hook_with};
use selflow::cli::commands::claude_hook_with;
use selflow::cli::{ClaudeAction, Cli, Command, HookEvent};
use selflow_protocol::ipc::{
    AgentKind, AgentSession, HELLO_MAGIC, Request, Response, ToolCall, ToolVerdict,
};
use serde_json::{Value, json};
use support::FakeDaemon;
use tempfile::TempDir;

const SESSION: &str = "b6e272b7-6796-4a56-8a9c-77257b60645f";
const CWD: &str = "/home/user/project";
const INBOX: &str = "/run/user/1000/cc-socks/42.sock";

fn fixture(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude");
    std::fs::read_to_string(dir.join(name)).unwrap()
}

fn input(name: &str) -> Value {
    serde_json::from_str(&fixture(name)).unwrap()
}

fn hook_env(attended: Option<&str>, entrypoint: Option<&str>) -> HookEnv {
    HookEnv {
        inbox: Some(INBOX.to_owned()),
        attended: attended.map(str::to_owned),
        entrypoint: entrypoint.map(str::to_owned),
    }
}

fn interactive() -> HookEnv {
    hook_env(Some("1"), Some("cli"))
}

fn session(attended: bool) -> AgentSession {
    AgentSession {
        agent: AgentKind::ClaudeCode,
        id: SESSION.to_owned(),
        cwd: CWD.to_owned(),
        inbox: Some(INBOX.to_owned()),
        attended,
    }
}

fn tool_request(tool: ToolCall) -> Request {
    Request::AgentTool {
        session: session(true),
        tool,
    }
}

fn parsed(output: Option<String>) -> Value {
    serde_json::from_str(&output.expect("the hook printed nothing")).unwrap()
}

#[test]
fn each_fixture_becomes_its_request() {
    let env = interactive();
    let request = |event, input: &Value| hook_request(event, input, &env);
    assert_eq!(
        request(HookEvent::SessionStart, &input("session-start.json")),
        Some(Request::AgentSessionStarted(session(true)))
    );
    assert_eq!(
        request(HookEvent::Prompt, &input("prompt.json")),
        Some(Request::AgentPrompt(session(true)))
    );
    assert_eq!(
        request(HookEvent::Tool, &input("tool-bash.json")),
        Some(tool_request(ToolCall::Shell {
            command: "git status".to_owned()
        }))
    );
    assert_eq!(
        request(HookEvent::Tool, &input("tool-write.json")),
        Some(tool_request(ToolCall::Edit))
    );
    let mut read = input("tool-write.json");
    read["tool_name"] = json!("Read");
    assert_eq!(
        request(HookEvent::Tool, &read),
        Some(tool_request(ToolCall::Other))
    );
    assert_eq!(
        request(HookEvent::SessionEnd, &input("session-end.json")),
        Some(Request::AgentSessionEnded {
            agent: AgentKind::ClaudeCode,
            id: SESSION.to_owned(),
        })
    );
}

#[test]
fn attended_follows_the_recorded_signal() {
    assert!(attended(&interactive()));
    let print_mode = hook_env(Some("0"), Some("sdk-cli"));
    assert!(!attended(&print_mode));
    assert!(attended(&hook_env(None, None)));
    assert_eq!(
        hook_request(HookEvent::Prompt, &input("prompt.json"), &print_mode),
        Some(Request::AgentPrompt(session(false)))
    );
}

#[test]
fn a_context_answer_becomes_additional_context() {
    let answer = Response::AgentContext {
        text: Some("Wrap up \"the\" work.".to_owned()),
    };
    assert_eq!(
        parsed(hook_output(HookEvent::Prompt, &answer)),
        json!({
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": "Wrap up \"the\" work."
            }
        })
    );
}

#[test]
fn no_text_prints_nothing() {
    let answer = Response::AgentContext { text: None };
    assert_eq!(hook_output(HookEvent::Prompt, &answer), None);
    assert_eq!(
        hook_output(HookEvent::SessionStart, &Response::AgentNoted),
        None
    );
}

#[test]
fn a_denial_becomes_a_pre_tool_use_deny() {
    let reason = "It's past bedtime: save your work.";
    let daemon = FakeDaemon::new([Response::AgentVerdict(ToolVerdict::Deny {
        reason: reason.to_owned(),
    })]);
    let mut out = Vec::new();
    let code = claude_hook_with(
        &[OsString::from("tool")],
        Cursor::new(fixture("tool-write.json")),
        &interactive(),
        Some(daemon.path()),
        &mut out,
    );
    assert_eq!(code, ExitCode::SUCCESS);
    let output = String::from_utf8(out).unwrap();
    assert_eq!(output.lines().count(), 1);
    assert_eq!(
        parsed(Some(output)),
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason
            }
        })
    );
    assert_eq!(daemon.requests(), [tool_request(ToolCall::Edit)]);
}

#[test]
fn an_allow_prints_nothing() {
    let answer = Response::AgentVerdict(ToolVerdict::Allow);
    assert_eq!(hook_output(HookEvent::Tool, &answer), None);
}

fn tool_hook(socket: Option<&Path>) -> Option<String> {
    hook_with(
        HookEvent::Tool,
        Cursor::new(fixture("tool-bash.json")),
        &interactive(),
        socket,
    )
}

fn socket_in(dir: &TempDir) -> PathBuf {
    dir.path().join("daemon.sock")
}

#[test]
fn an_unreachable_daemon_lets_the_call_through() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(tool_hook(Some(&socket_in(&dir))), None);
    assert_eq!(tool_hook(None), None);
}

#[test]
fn a_daemon_that_never_answers_lets_the_call_through_in_time() {
    let dir = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(socket_in(&dir)).unwrap();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_secs(3));
        drop(stream);
    });
    let started = Instant::now();
    assert_eq!(tool_hook(Some(&socket_in(&dir))), None);
    assert!(started.elapsed() < Duration::from_millis(1500));
}

#[test]
fn a_daemon_on_another_version_lets_the_call_through() {
    let dir = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(socket_in(&dir)).unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut hello = [0; 8];
        stream.read_exact(&mut hello).unwrap();
        let mut answer = HELLO_MAGIC.to_vec();
        answer.extend_from_slice(&999u32.to_le_bytes());
        stream.write_all(&answer).unwrap();
        let _ = stream.read_to_end(&mut Vec::new());
    });
    assert_eq!(tool_hook(Some(&socket_in(&dir))), None);
}

#[test]
fn input_that_is_not_json_lets_the_call_through() {
    let daemon = FakeDaemon::new([Response::AgentVerdict(ToolVerdict::Deny {
        reason: "no".to_owned(),
    })]);
    let output = hook_with(
        HookEvent::Tool,
        Cursor::new("not json"),
        &interactive(),
        Some(daemon.path()),
    );
    assert_eq!(output, None);
    assert!(daemon.requests().is_empty());
}

#[test]
fn stdin_that_never_closes_lets_the_call_through_in_time() {
    let daemon = FakeDaemon::new([Response::AgentVerdict(ToolVerdict::Deny {
        reason: "no".to_owned(),
    })]);
    let (reader, writer) = std::io::pipe().unwrap();
    let started = Instant::now();
    let output = hook_with(HookEvent::Tool, reader, &interactive(), Some(daemon.path()));
    assert!(started.elapsed() < Duration::from_millis(1500));
    assert_eq!(output, None);
    assert!(daemon.requests().is_empty());
    drop(writer);
}

fn hook_args<I: Into<OsString> + Clone>(args: impl IntoIterator<Item = I>) -> Vec<OsString> {
    let line = ["selflow", "claude", "hook"]
        .into_iter()
        .map(OsString::from)
        .chain(args.into_iter().map(Into::into));
    let Command::Claude(claude) = Cli::try_parse_from(line).unwrap().command else {
        panic!("not the claude command");
    };
    let ClaudeAction::Hook { args } = claude.action else {
        panic!("not the hook action");
    };
    args
}

#[test]
fn the_hook_reads_its_event_from_any_arguments() {
    assert_eq!(hook_event(&hook_args(["tool"])), Some(HookEvent::Tool));
    assert_eq!(
        hook_event(&hook_args(["session-start"])),
        Some(HookEvent::SessionStart)
    );
    for args in [
        &["bogus"][..],
        &[],
        &["tool", "extra"],
        &["--help"],
        &["-x", "tool"],
    ] {
        assert_eq!(
            hook_event(&hook_args(args.iter().copied())),
            None,
            "{args:?}"
        );
    }
    let not_utf8 = OsString::from_vec(vec![b't', 0xff]);
    assert_eq!(hook_event(&hook_args([not_utf8])), None);
}

#[test]
fn an_unknown_event_exits_0_without_output() {
    let daemon = FakeDaemon::new([Response::AgentVerdict(ToolVerdict::Deny {
        reason: "no".to_owned(),
    })]);
    let mut out = Vec::new();
    let code = claude_hook_with(
        &hook_args(["bogus"]),
        Cursor::new(fixture("tool-bash.json")),
        &interactive(),
        Some(daemon.path()),
        &mut out,
    );
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(out.is_empty());
    assert!(daemon.requests().is_empty());
}
