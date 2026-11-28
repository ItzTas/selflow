mod constants;
mod types;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use jiff::{Timestamp, ZonedDateTime, tz::TimeZone};
use selflow_core::agent::{ActiveAgentStep, command_allowed, covers, governing};
use selflow_core::config::AgentLevel;
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_protocol::ipc::{AgentKind, AgentSession, Response, ToolCall, ToolVerdict};
use tokio::sync::{Notify, watch};

use crate::engine::clock_time;
use crate::store::Store;

use constants::{COMMAND_SEPARATOR, PARAGRAPH_BREAK, PROMPT_DELIVERY};
pub use types::{AgentError, Directives};

type SessionKey = (AgentKind, String);

pub struct AgentHub {
    store: Arc<Mutex<Store>>,
    directives: watch::Receiver<Directives>,
    home: Option<PathBuf>,
    sessions: Mutex<HashMap<SessionKey, AgentSession>>,
    localizers: Mutex<HashMap<Language, Localizer>>,
    changed: Notify,
}

impl AgentHub {
    pub fn new(
        store: Arc<Mutex<Store>>,
        directives: watch::Receiver<Directives>,
        home: Option<PathBuf>,
    ) -> AgentHub {
        AgentHub {
            store,
            directives,
            home,
            sessions: Mutex::new(HashMap::new()),
            localizers: Mutex::new(HashMap::new()),
            changed: Notify::new(),
        }
    }

    pub fn started(&self, session: AgentSession, _now: &ZonedDateTime) -> Response {
        self.register(session);
        Response::AgentNoted
    }

    pub fn ended(&self, agent: AgentKind, id: &str, _now: &ZonedDateTime) -> Response {
        guarded(&self.sessions).remove(&(agent, id.to_owned()));
        Response::AgentNoted
    }

    pub fn prompt(&self, session: AgentSession, now: &ZonedDateTime) -> Response {
        self.register(session.clone());
        let directives = self.directives.borrow().clone();
        let claimed = self.claim_unsent(
            &session,
            &directives.steps,
            PROMPT_DELIVERY,
            now.timestamp(),
        );
        let reminded = governing(
            &directives.steps,
            &lexically_normal(&session.cwd),
            self.home.as_deref(),
            session.attended,
        )
        .filter(|step| step.step.level >= AgentLevel::Light)
        .filter(|step| !claimed.iter().any(|claimed| claimed.key == step.key));
        let texts = self.with_texts(&directives, now.time_zone(), |texts| {
            claimed
                .iter()
                .map(|step| texts.message(step))
                .chain(reminded.map(|step| texts.reminder(step)))
                .collect::<Vec<String>>()
        });
        Response::AgentContext {
            text: (!texts.is_empty()).then(|| texts.join(PARAGRAPH_BREAK)),
        }
    }

    pub fn tool(&self, session: AgentSession, tool: ToolCall, now: &ZonedDateTime) -> Response {
        let directives = self.directives.borrow().clone();
        let save_only = governing(
            &directives.steps,
            &lexically_normal(&session.cwd),
            self.home.as_deref(),
            session.attended,
        )
        .filter(|step| step.step.level == AgentLevel::SaveOnly);
        let Some(step) = save_only.filter(|_| !tool_allowed(&tool, &directives.allowed_commands))
        else {
            return Response::AgentVerdict(ToolVerdict::Allow);
        };
        let reason = self.with_texts(&directives, now.time_zone(), |texts| texts.denied(step));
        Response::AgentVerdict(ToolVerdict::Deny { reason })
    }

    pub fn sessions(&self) -> Vec<AgentSession> {
        guarded(&self.sessions).values().cloned().collect()
    }

    pub fn changed(&self) -> &Notify {
        &self.changed
    }

    fn register(&self, session: AgentSession) {
        let key = (session.agent, session.id.clone());
        let mut sessions = guarded(&self.sessions);
        sessions.insert(key.clone(), session);
        sessions.retain(|known, session| *known == key || inbox_present(session));
        drop(sessions);
        self.changed.notify_one();
    }

    fn claim_unsent(
        &self,
        session: &AgentSession,
        steps: &[ActiveAgentStep],
        how: &str,
        at: Timestamp,
    ) -> Vec<ActiveAgentStep> {
        let cwd = lexically_normal(&session.cwd);
        let mut covering: Vec<&ActiveAgentStep> = steps
            .iter()
            .filter(|active| session.attended || active.step.unattended)
            .filter(|active| covers(&active.step, &cwd, self.home.as_deref()))
            .collect();
        if covering.is_empty() {
            return Vec::new();
        }
        covering.sort_by_key(|active| active.at);
        let store = guarded(&self.store);
        covering
            .into_iter()
            .filter(|active| claimed_now(&store, active, &session.id, at, how))
            .cloned()
            .collect()
    }

    fn with_texts<T>(
        &self,
        directives: &Directives,
        tz: &TimeZone,
        render: impl FnOnce(&Texts) -> T,
    ) -> T {
        let mut localizers = guarded(&self.localizers);
        let language = directives.language;
        let localizer = localizers
            .entry(language)
            .or_insert_with(|| Localizer::new(language));
        render(&Texts {
            localizer,
            commands: directives.allowed_commands.join(COMMAND_SEPARATOR),
            tz,
        })
    }
}

fn claimed_now(
    store: &Store,
    active: &ActiveAgentStep,
    session_id: &str,
    at: Timestamp,
    how: &str,
) -> bool {
    store
        .claim_agent_delivery(&active.key, session_id, at, how)
        .map_err(AgentError::from)
        .unwrap_or_else(|error| {
            tracing::error!(%error, step = active.key, "couldn't claim an agent message");
            false
        })
}

fn tool_allowed(tool: &ToolCall, allowed: &[String]) -> bool {
    match tool {
        ToolCall::Edit => false,
        ToolCall::Shell { command } => command_allowed(command, allowed),
        ToolCall::Other => true,
    }
}

fn inbox_present(session: &AgentSession) -> bool {
    session
        .inbox
        .as_deref()
        .is_none_or(|inbox| Path::new(inbox).exists())
}

fn lexically_normal(cwd: &str) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in Path::new(cwd).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

struct Texts<'a> {
    localizer: &'a Localizer,
    commands: String,
    tz: &'a TimeZone,
}

impl Texts<'_> {
    fn message(&self, active: &ActiveAgentStep) -> String {
        let [label, time] = self.rule_args(active);
        let level = active.step.level;
        let text = active.message.clone().unwrap_or_else(|| {
            self.localizer.text(
                &format!("agent-text-{}", level.name()),
                &[label.clone(), time.clone()],
            )
        });
        let text = ("text", Arg::Text(text));
        if level == AgentLevel::Message {
            return self
                .localizer
                .text("agent-message-plain", &[label, time, text]);
        }
        let instruction = ("instruction", Arg::Text(self.instruction(active)));
        self.localizer
            .text("agent-message", &[label, time, text, instruction])
    }

    fn reminder(&self, active: &ActiveAgentStep) -> String {
        let [label, _] = self.rule_args(active);
        let instruction = ("instruction", Arg::Text(self.instruction(active)));
        self.localizer.text("agent-reminder", &[label, instruction])
    }

    fn denied(&self, active: &ActiveAgentStep) -> String {
        let [label, _] = self.rule_args(active);
        self.localizer.text(
            "agent-denied",
            &[label, self.until(active), self.commands()],
        )
    }

    fn instruction(&self, active: &ActiveAgentStep) -> String {
        self.localizer.text(
            &format!("agent-instruction-{}", active.step.level.name()),
            &[self.until(active), self.commands()],
        )
    }

    fn rule_args(&self, active: &ActiveAgentStep) -> [(&'static str, Arg); 2] {
        let window = &active.window;
        [
            ("label", Arg::Text(window.label.clone())),
            (
                "time",
                Arg::Text(clock_time(window.starts_at.timestamp(), self.tz)),
            ),
        ]
    }

    fn until(&self, active: &ActiveAgentStep) -> (&'static str, Arg) {
        (
            "until",
            Arg::Text(clock_time(active.window.ends_at.timestamp(), self.tz)),
        )
    }

    fn commands(&self) -> (&'static str, Arg) {
        ("commands", Arg::Text(self.commands.clone()))
    }
}

fn guarded<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
