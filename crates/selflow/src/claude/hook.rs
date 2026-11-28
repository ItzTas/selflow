use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use clap::ValueEnum;

use selflow_client::client::{NeverRestart, exchange_with};
use selflow_protocol::ipc::{AgentKind, AgentSession, Request, Response, ToolCall, ToolVerdict};
use serde_json::{Value, json};
use tokio::runtime::Builder;
use tokio::time::timeout;

use super::constants::{EDIT_TOOLS, HOOK_DEADLINE};
use super::types::{HookEnv, HookEvent};

pub fn hook_env() -> HookEnv {
    let read = |name: &str| std::env::var(name).ok();
    HookEnv {
        inbox: read("CLAUDE_CODE_MESSAGING_SOCKET"),
        attended: read("CLAUDE_CODE_SESSION_ATTENDED"),
        entrypoint: read("CLAUDE_CODE_ENTRYPOINT"),
    }
}

pub fn hook_event(args: &[OsString]) -> Option<HookEvent> {
    let [arg] = args else {
        return None;
    };
    HookEvent::from_str(arg.to_str()?, false).ok()
}

pub fn attended(env: &HookEnv) -> bool {
    env.attended.as_deref() != Some("0")
}

pub fn hook_request(event: HookEvent, input: &Value, env: &HookEnv) -> Option<Request> {
    let id = input["session_id"].as_str()?.to_owned();
    match event {
        HookEvent::SessionStart => session(id, input, env).map(Request::AgentSessionStarted),
        HookEvent::SessionEnd => Some(Request::AgentSessionEnded {
            agent: AgentKind::ClaudeCode,
            id,
        }),
        HookEvent::Prompt => session(id, input, env).map(Request::AgentPrompt),
        HookEvent::Tool => Some(Request::AgentTool {
            session: session(id, input, env)?,
            tool: tool_call(input)?,
        }),
    }
}

fn session(id: String, input: &Value, env: &HookEnv) -> Option<AgentSession> {
    Some(AgentSession {
        agent: AgentKind::ClaudeCode,
        id,
        cwd: input["cwd"].as_str()?.to_owned(),
        inbox: env.inbox.clone(),
        attended: attended(env),
    })
}

fn tool_call(input: &Value) -> Option<ToolCall> {
    let name = input["tool_name"].as_str()?;
    if EDIT_TOOLS.contains(&name) {
        return Some(ToolCall::Edit);
    }
    let Some(command) = input["tool_input"]["command"].as_str() else {
        return Some(ToolCall::Other);
    };
    Some(ToolCall::Shell {
        command: command.to_owned(),
    })
}

pub fn hook_output(event: HookEvent, response: &Response) -> Option<String> {
    let name = event.settings_event();
    let output = match (event, response) {
        (HookEvent::Prompt, Response::AgentContext { text: Some(text) }) => json!({
            "hookSpecificOutput": {"hookEventName": name, "additionalContext": text}
        }),
        (HookEvent::Tool, Response::AgentVerdict(ToolVerdict::Deny { reason })) => json!({
            "hookSpecificOutput": {
                "hookEventName": name,
                "permissionDecision": "deny",
                "permissionDecisionReason": reason
            }
        }),
        _ => return None,
    };
    Some(output.to_string())
}

pub fn hook_with(
    event: HookEvent,
    input: impl Read + Send + 'static,
    env: &HookEnv,
    socket: Option<&Path>,
) -> Option<String> {
    let started = Instant::now();
    let input: Value = serde_json::from_slice(&read_within(input, HOOK_DEADLINE)?).ok()?;
    let request = hook_request(event, &input, env)?;
    let socket = socket?;
    let left = HOOK_DEADLINE.checked_sub(started.elapsed())?;
    let runtime = Builder::new_current_thread().enable_all().build().ok()?;
    let exchange = exchange_with(socket, &request, &NeverRestart);
    let response = runtime
        .block_on(async { timeout(left, exchange).await })
        .ok()?
        .ok()?;
    hook_output(event, &response)
}

fn read_within(mut input: impl Read + Send + 'static, limit: Duration) -> Option<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .spawn(move || {
            let mut bytes = Vec::new();
            let _ = sender.send(input.read_to_end(&mut bytes).map(|_| bytes));
        })
        .ok()?;
    receiver.recv_timeout(limit).ok()?.ok()
}
