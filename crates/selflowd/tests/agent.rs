mod support;

use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use jiff::ZonedDateTime;
use selflow_protocol::ipc::{AgentSession, MessageView, Request, Response, ToolCall, ToolVerdict};
use selflowd::agent::claude_code::inbox_frame;
use selflowd::agent::courier::{restarting, run_courier};
use selflowd::ipc::handle;
use selflowd::store::{Event, EventKind};
use support::{AgentRig, agent_rig, agent_rig_on, at, no_jitter, session};
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

const LIGHT: &str = "before = \"60m\"\nlevel = \"light\"";

const WIND_DOWN: &str = "Message from Selflow, the user's own focus tool, for the bedtime rule at 23:00: Time to wind down: bedtime starts at 23:00. Until 07:00, finish what is underway and take on only small tasks. Please pass this message on to the user.";

const LIGHT_REMINDER: &str = "Reminder from Selflow, the user's own focus tool (bedtime rule): Until 07:00, finish what is underway and take on only small tasks.";

fn bedtime_with(steps: &[&str]) -> String {
    let tables: String = steps
        .iter()
        .map(|body| format!("\n[[rule_set.rule.agent]]\n{body}\n"))
        .collect();
    let english = no_jitter().replace("language = \"pt-BR\"", "language = \"en\"");
    format!("{english}{tables}")
}

fn ticked(steps: &[&str], now: ZonedDateTime) -> AgentRig {
    let mut rig = agent_rig(&bedtime_with(steps), now);
    rig.engine.tick().unwrap();
    rig
}

fn prompt(rig: &AgentRig, session: AgentSession) -> Option<String> {
    match handle(Request::AgentPrompt(session), &rig.shared) {
        Response::AgentContext { text } => text,
        other => panic!("expected agent context, got {other:?}"),
    }
}

fn verdict(rig: &AgentRig, session: AgentSession, tool: ToolCall) -> ToolVerdict {
    match handle(Request::AgentTool { session, tool }, &rig.shared) {
        Response::AgentVerdict(verdict) => verdict,
        other => panic!("expected a verdict, got {other:?}"),
    }
}

fn report(rig: &AgentRig) -> Vec<MessageView> {
    let request = Request::Report {
        logical_date: Some(jiff::civil::date(2026, 11, 19)),
    };
    match handle(request, &rig.shared) {
        Response::Report(view) => view
            .entries
            .into_iter()
            .map(|entry| entry.message)
            .collect(),
        other => panic!("expected a report, got {other:?}"),
    }
}

fn agent_lines(rig: &AgentRig) -> Vec<MessageView> {
    report(rig)
        .into_iter()
        .filter(|message| message.id.starts_with("report-agent-"))
        .collect()
}

fn line(id: &str, level: &str, folder: &str) -> MessageView {
    MessageView {
        id: id.to_owned(),
        args: vec![
            ("label".to_owned(), "bedtime".to_owned()),
            ("level".to_owned(), level.to_owned()),
            ("folder".to_owned(), folder.to_owned()),
        ],
    }
}

fn shell(command: &str) -> ToolCall {
    ToolCall::Shell {
        command: command.to_owned(),
    }
}

fn seconv() -> AgentSession {
    session("3f6c1d52", "/home/user/Workspace/seconv")
}

fn with_inbox(inbox: &Path) -> AgentSession {
    AgentSession {
        inbox: Some(inbox.to_string_lossy().into_owned()),
        ..seconv()
    }
}

fn courier(rig: &AgentRig) -> JoinHandle<()> {
    tokio::spawn(run_courier(
        Arc::clone(&rig.shared.agents),
        rig.clock.clone(),
    ))
}

async fn received(listener: &UnixListener) -> Vec<u8> {
    let wait = Duration::from_secs(5);
    let (mut stream, _) = timeout(wait, listener.accept()).await.unwrap().unwrap();
    let mut frame = Vec::new();
    timeout(wait, stream.read_to_end(&mut frame))
        .await
        .unwrap()
        .unwrap();
    frame
}

async fn eventually<T: PartialEq + Debug>(current: impl Fn() -> T, expected: T) {
    for _ in 0..500 {
        if current() == expected {
            return;
        }
        sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(current(), expected);
}

async fn assert_next_prompt_gets_the_message(rig: &AgentRig, session: AgentSession) {
    let courier = courier(rig);
    handle(Request::AgentSessionStarted(session.clone()), &rig.shared);
    eventually(|| rig.shared.agents.sessions(), Vec::new()).await;
    assert_eq!(prompt(rig, session).as_deref(), Some(WIND_DOWN));
    courier.abort();
}

#[test]
fn a_prompt_gets_the_message_once_then_the_reminder() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    assert_eq!(prompt(&rig, seconv()).as_deref(), Some(WIND_DOWN));
    assert_eq!(prompt(&rig, seconv()).as_deref(), Some(LIGHT_REMINDER));
}

#[test]
fn a_message_only_step_sends_no_reminder() {
    let rig = ticked(&["before = \"30m\"\nlevel = \"message\""], at(19, 22, 40));
    assert_eq!(
        prompt(&rig, seconv()).as_deref(),
        Some(
            "Message from Selflow, the user's own focus tool, for the bedtime rule at 23:00: bedtime starts at 23:00. Please pass this message on to the user."
        )
    );
    assert_eq!(prompt(&rig, seconv()), None);
}

#[test]
fn a_step_with_its_own_message_sends_that_text() {
    let rig = ticked(
        &["before = \"60m\"\nlevel = \"no-new-work\"\nmessage = \"Commit and go to bed.\""],
        at(19, 22, 10),
    );
    assert_eq!(
        prompt(&rig, seconv()).as_deref(),
        Some(
            "Message from Selflow, the user's own focus tool, for the bedtime rule at 23:00: Commit and go to bed. Until 07:00, take on nothing new. Help save the work, write down where it stopped and what the next step is, and tell the user to go rest. Please pass this message on to the user."
        )
    );
}

#[test]
fn a_session_outside_the_folders_gets_nothing() {
    let rig = ticked(
        &[&format!("{LIGHT}\nfolders = [\"~/Workspace/seconv\"]")],
        at(19, 22, 10),
    );
    assert_eq!(prompt(&rig, session("7a1b", "/home/user/notes")), None);
    assert_eq!(
        prompt(&rig, session("7a1b", "/home/user/Workspace/seconv-old")),
        None
    );
    assert_eq!(
        prompt(
            &rig,
            session("9c2d", "/home/user/notes/../Workspace/seconv/src")
        )
        .as_deref(),
        Some(WIND_DOWN)
    );
}

#[test]
fn save_only_denies_edits_and_other_commands() {
    let rig = ticked(&["before = \"60m\"\nlevel = \"save-only\""], at(19, 22, 10));
    assert_eq!(
        verdict(&rig, seconv(), ToolCall::Edit),
        ToolVerdict::Deny {
            reason: "Selflow blocks this until 07:00 (bedtime rule). Only these commands can run: git status, git diff, git add, git commit, git stash push. Help the user save the work and stop for the day.".to_owned()
        }
    );
    assert_eq!(
        verdict(&rig, seconv(), shell("git commit -m save")),
        ToolVerdict::Allow
    );
    assert!(matches!(
        verdict(&rig, seconv(), shell("rm -rf src")),
        ToolVerdict::Deny { .. }
    ));
    assert!(matches!(
        verdict(&rig, seconv(), shell("git commit -m x && rm -rf src")),
        ToolVerdict::Deny { .. }
    ));
    assert_eq!(verdict(&rig, seconv(), ToolCall::Other), ToolVerdict::Allow);
}

#[test]
fn levels_below_save_only_allow_every_tool() {
    let rig = ticked(
        &["before = \"60m\"\nlevel = \"no-new-work\""],
        at(19, 22, 10),
    );
    assert_eq!(verdict(&rig, seconv(), ToolCall::Edit), ToolVerdict::Allow);
    assert_eq!(
        verdict(&rig, seconv(), shell("rm -rf src")),
        ToolVerdict::Allow
    );
}

#[test]
fn the_firmest_level_wins() {
    let rig = ticked(
        &[
            LIGHT,
            "before = \"30m\"\nlevel = \"save-only\"\nfolders = [\"~/Workspace/work\"]",
        ],
        at(19, 22, 40),
    );
    assert!(matches!(
        verdict(
            &rig,
            session("1", "/home/user/Workspace/work"),
            ToolCall::Edit
        ),
        ToolVerdict::Deny { .. }
    ));
    assert_eq!(
        verdict(&rig, session("2", "/home/user/notes"), ToolCall::Edit),
        ToolVerdict::Allow
    );
}

#[test]
fn the_level_ends_with_the_rule() {
    let rig = ticked(&["before = \"60m\"\nlevel = \"save-only\""], at(20, 7, 0));
    assert_eq!(prompt(&rig, seconv()), None);
    assert_eq!(verdict(&rig, seconv(), ToolCall::Edit), ToolVerdict::Allow);
}

#[test]
fn a_restart_does_not_resend_a_delivered_message() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    assert_eq!(prompt(&rig, seconv()).as_deref(), Some(WIND_DOWN));
    let mut restarted = agent_rig_on(rig.store.clone(), at(19, 22, 10));
    restarted.engine.tick().unwrap();
    assert_eq!(
        prompt(&restarted, seconv()).as_deref(),
        Some(LIGHT_REMINDER)
    );
}

#[test]
fn an_inbox_claim_without_its_message_is_released_when_the_daemon_starts() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    let key = "bedtime/2026-11-19/60m/light";
    let sent = session("8e4f", "/home/user/Workspace/seconv");
    let lost = seconv();
    let claimed_at = at(19, 22, 11).timestamp();
    {
        let store = rig.store.lock().unwrap();
        for id in [&sent.id, &lost.id] {
            assert!(
                store
                    .claim_agent_delivery(key, id, claimed_at, "inbox")
                    .unwrap()
            );
        }
        store
            .record_event(&Event {
                at: claimed_at,
                logical_date: jiff::civil::date(2026, 11, 19),
                kind: EventKind::AgentMessage,
                key: format!("{key}/{}", sent.id),
                detail: None,
            })
            .unwrap();
    }
    let mut restarted = agent_rig_on(rig.store.clone(), at(19, 22, 12));
    restarted.engine.tick().unwrap();
    assert_eq!(prompt(&restarted, lost).as_deref(), Some(WIND_DOWN));
    assert_eq!(prompt(&restarted, sent).as_deref(), Some(LIGHT_REMINDER));
}

#[test]
fn a_session_that_registers_after_the_step_fired_gets_the_message() {
    let mut rig = ticked(&[LIGHT], at(19, 22, 10));
    assert_eq!(prompt(&rig, seconv()).as_deref(), Some(WIND_DOWN));
    rig.clock.set(at(19, 22, 30));
    rig.engine.tick().unwrap();
    let later = session("8e4f", "/home/user/Workspace/seconv");
    assert_eq!(
        handle(Request::AgentSessionStarted(later.clone()), &rig.shared),
        Response::AgentNoted
    );
    assert_eq!(prompt(&rig, later).as_deref(), Some(WIND_DOWN));
}

#[test]
fn an_unattended_session_gets_nothing_unless_the_step_says_so() {
    let unattended = AgentSession {
        attended: false,
        ..seconv()
    };
    let rig = ticked(&["before = \"60m\"\nlevel = \"save-only\""], at(19, 22, 10));
    assert_eq!(prompt(&rig, unattended.clone()), None);
    assert_eq!(
        verdict(&rig, unattended.clone(), ToolCall::Edit),
        ToolVerdict::Allow
    );
    let rig = ticked(
        &["before = \"60m\"\nlevel = \"save-only\"\nunattended = true"],
        at(19, 22, 10),
    );
    assert!(
        prompt(&rig, unattended.clone())
            .is_some_and(|text| text.contains("Time to stop: bedtime starts at 23:00."))
    );
    assert!(matches!(
        verdict(&rig, unattended, ToolCall::Edit),
        ToolVerdict::Deny { .. }
    ));
}

#[test]
fn sessions_are_registered_until_they_end() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    let started = session("1", "/home/user/a");
    handle(Request::AgentSessionStarted(started.clone()), &rig.shared);
    prompt(&rig, session("2", "/home/user/b"));
    assert_eq!(rig.shared.agents.sessions().len(), 2);
    assert_eq!(
        handle(
            Request::AgentSessionEnded {
                agent: started.agent,
                id: started.id.clone(),
            },
            &rig.shared
        ),
        Response::AgentNoted
    );
    assert_eq!(
        rig.shared.agents.sessions(),
        vec![session("2", "/home/user/b")]
    );
}

#[test]
fn a_session_whose_inbox_is_gone_is_dropped() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    let dir = tempfile::tempdir().unwrap();
    let inbox = dir.path().join("4242.sock");
    std::fs::write(&inbox, "").unwrap();
    let gone = AgentSession {
        inbox: Some(inbox.to_string_lossy().into_owned()),
        ..session("1", "/home/user/a")
    };
    handle(Request::AgentSessionStarted(gone), &rig.shared);
    std::fs::remove_file(&inbox).unwrap();
    let other = session("2", "/home/user/b");
    handle(Request::AgentSessionStarted(other.clone()), &rig.shared);
    assert_eq!(rig.shared.agents.sessions(), vec![other]);
}

#[test]
fn a_message_delivered_with_a_prompt_is_reported() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    prompt(&rig, seconv());
    prompt(&rig, seconv());
    assert_eq!(
        agent_lines(&rig),
        vec![line(
            "report-agent-message-prompt",
            "light",
            "~/Workspace/seconv"
        )]
    );
}

#[test]
fn a_denial_is_reported_without_the_command() {
    let rig = ticked(&["before = \"60m\"\nlevel = \"save-only\""], at(19, 22, 10));
    verdict(&rig, seconv(), shell("rm -rf secret-folder"));
    verdict(&rig, seconv(), ToolCall::Edit);
    let lines = agent_lines(&rig);
    assert_eq!(
        lines,
        vec![
            line(
                "report-agent-denied-shell",
                "save-only",
                "~/Workspace/seconv"
            ),
            line(
                "report-agent-denied-edit",
                "save-only",
                "~/Workspace/seconv"
            ),
        ]
    );
    assert!(!format!("{lines:?}").contains("secret-folder"));
    let stored = rig
        .store
        .lock()
        .unwrap()
        .events_for(jiff::civil::date(2026, 11, 19))
        .unwrap();
    assert!(!format!("{stored:?}").contains("secret-folder"));
}

#[test]
fn each_kind_of_denied_call_is_reported_once_per_session() {
    let rig = ticked(&["before = \"60m\"\nlevel = \"save-only\""], at(19, 22, 10));
    for command in ["rm a", "rm b", "touch c"] {
        verdict(&rig, seconv(), shell(command));
    }
    assert_eq!(
        agent_lines(&rig),
        vec![line(
            "report-agent-denied-shell",
            "save-only",
            "~/Workspace/seconv"
        )]
    );
    verdict(&rig, seconv(), ToolCall::Edit);
    verdict(&rig, session("other", "/home/user/a"), ToolCall::Edit);
    assert_eq!(agent_lines(&rig).len(), 3);
}

#[test]
fn a_session_that_ends_first_is_reported_as_missed() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    let quiet = session("1", "/home/user/a");
    handle(Request::AgentSessionStarted(quiet.clone()), &rig.shared);
    handle(
        Request::AgentSessionEnded {
            agent: quiet.agent,
            id: quiet.id,
        },
        &rig.shared,
    );
    let served = seconv();
    prompt(&rig, served.clone());
    handle(
        Request::AgentSessionEnded {
            agent: served.agent,
            id: served.id,
        },
        &rig.shared,
    );
    assert_eq!(
        agent_lines(&rig),
        vec![
            line("report-agent-message-missed", "light", "~/a"),
            line("report-agent-message-prompt", "light", "~/Workspace/seconv"),
        ]
    );
}

#[test]
fn an_unattended_run_during_a_level_is_reported_once() {
    let rig = ticked(&["before = \"60m\"\nlevel = \"save-only\""], at(19, 22, 10));
    let unattended = AgentSession {
        attended: false,
        ..seconv()
    };
    prompt(&rig, unattended.clone());
    prompt(&rig, unattended);
    assert_eq!(
        agent_lines(&rig),
        vec![line(
            "report-agent-unattended",
            "save-only",
            "~/Workspace/seconv"
        )]
    );
}

#[test]
fn an_unattended_run_a_step_governs_is_reported_as_a_message() {
    let rig = ticked(
        &["before = \"60m\"\nlevel = \"save-only\"\nunattended = true"],
        at(19, 22, 10),
    );
    let unattended = AgentSession {
        attended: false,
        ..seconv()
    };
    prompt(&rig, unattended);
    assert_eq!(
        agent_lines(&rig),
        vec![line(
            "report-agent-message-prompt",
            "save-only",
            "~/Workspace/seconv"
        )]
    );
}

#[test]
fn an_unattended_run_one_covering_step_governs_is_not_reported_as_ungoverned() {
    let rig = ticked(
        &[
            "before = \"60m\"\nlevel = \"save-only\"\nunattended = true",
            "before = \"30m\"\nlevel = \"light\"",
        ],
        at(19, 22, 40),
    );
    let unattended = AgentSession {
        attended: false,
        ..seconv()
    };
    prompt(&rig, unattended);
    let ids: Vec<String> = agent_lines(&rig).into_iter().map(|line| line.id).collect();
    assert_eq!(ids, vec!["report-agent-message-prompt"]);
}

#[test]
fn the_frame_follows_the_recorded_framing_with_the_session_id() {
    assert_eq!(
        inbox_frame("3f6c1d52", "hello from selflow"),
        b"{\"type\":\"user\",\"session_id\":\"3f6c1d52\",\"message\":{\"role\":\"user\",\"content\":\"hello from selflow\"}}\n"
    );
    let quoted = "say \"stop\"\nnow";
    let frame = inbox_frame("3f6c1d52", quoted);
    let parsed: serde_json::Value = serde_json::from_slice(&frame).unwrap();
    assert_eq!(parsed["session_id"], "3f6c1d52");
    assert_eq!(parsed["message"]["content"], quoted);
    assert_eq!(frame.iter().filter(|byte| **byte == b'\n').count(), 1);
}

#[tokio::test]
async fn an_idle_session_gets_the_message_through_its_inbox() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    let dir = tempfile::tempdir().unwrap();
    let inbox = dir.path().join("4242.sock");
    let listener = UnixListener::bind(&inbox).unwrap();
    let courier = courier(&rig);
    handle(
        Request::AgentSessionStarted(with_inbox(&inbox)),
        &rig.shared,
    );
    let frame = received(&listener).await;
    let parsed: serde_json::Value = serde_json::from_slice(&frame).unwrap();
    assert_eq!(parsed["session_id"], with_inbox(&inbox).id);
    assert_eq!(frame, inbox_frame(&with_inbox(&inbox).id, WIND_DOWN));
    assert_eq!(
        prompt(&rig, with_inbox(&inbox)).as_deref(),
        Some(LIGHT_REMINDER)
    );
    eventually(
        || agent_lines(&rig),
        vec![line(
            "report-agent-message-inbox",
            "light",
            "~/Workspace/seconv",
        )],
    )
    .await;
    courier.abort();
}

#[tokio::test]
async fn a_gone_inbox_leaves_the_message_for_the_next_prompt() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    let dir = tempfile::tempdir().unwrap();
    assert_next_prompt_gets_the_message(&rig, with_inbox(&dir.path().join("4242.sock"))).await;
}

#[tokio::test]
async fn a_refusing_inbox_leaves_the_message_for_the_next_prompt() {
    let rig = ticked(&[LIGHT], at(19, 22, 10));
    let dir = tempfile::tempdir().unwrap();
    let inbox = dir.path().join("4242.sock");
    drop(UnixListener::bind(&inbox).unwrap());
    assert!(inbox.exists());
    assert_next_prompt_gets_the_message(&rig, with_inbox(&inbox)).await;
}

#[tokio::test]
async fn the_courier_starts_again_after_it_panics_or_stops() {
    let starts = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&starts);
    let supervisor = tokio::spawn(restarting(Duration::from_millis(20), move || {
        let start = counted.fetch_add(1, Ordering::SeqCst);
        async move {
            match start {
                0 => panic!("a courier bug"),
                1 => {}
                _ => std::future::pending().await,
            }
        }
    }));
    eventually(|| starts.load(Ordering::SeqCst), 3).await;
    sleep(Duration::from_millis(100)).await;
    assert_eq!(starts.load(Ordering::SeqCst), 3);
    supervisor.abort();
}
