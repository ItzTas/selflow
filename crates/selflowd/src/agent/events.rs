use std::path::Path;

use jiff::Timestamp;
use selflow_core::agent::ActiveAgentStep;
use selflow_protocol::ipc::AgentSession;
use serde_json::json;

use crate::store::{Event, EventKind, Store};

use super::constants::HOME_SYMBOL;

pub(super) struct Origin<'a> {
    pub(super) active: &'a ActiveAgentStep,
    pub(super) session: &'a AgentSession,
    pub(super) cwd: &'a Path,
    pub(super) home: Option<&'a Path>,
    pub(super) at: Timestamp,
}

impl Origin<'_> {
    pub(super) fn message(&self, how: &str) -> Event {
        self.event(EventKind::AgentMessage, self.pair_key(), ("how", how))
    }

    pub(super) fn denied(&self, tool: &str) -> Event {
        let key = format!("{}/{}", self.pair_key(), self.at);
        self.event(EventKind::AgentDenied, key, ("tool", tool))
    }

    pub(super) fn unattended(&self) -> Event {
        self.event(
            EventKind::AgentUnattended,
            self.pair_key(),
            ("session", "unattended"),
        )
    }

    fn pair_key(&self) -> String {
        format!("{}/{}", self.active.key, self.session.id)
    }

    fn event(&self, kind: EventKind, key: String, extra: (&str, &str)) -> Event {
        let mut detail = json!({
            "label": self.active.window.label,
            "level": self.active.step.level.name(),
            "folder": shown_folder(self.cwd, self.home),
        });
        detail[extra.0] = json!(extra.1);
        Event {
            at: self.at,
            logical_date: self.active.window.logical_date,
            kind,
            key,
            detail: Some(detail.to_string()),
        }
    }
}

pub(super) fn record(store: &Store, event: &Event) {
    if let Err(error) = store.record_event(event) {
        tracing::error!(%error, ?event, "couldn't record an agent event");
    }
}

fn shown_folder(cwd: &Path, home: Option<&Path>) -> String {
    let Some(rest) = home.and_then(|home| cwd.strip_prefix(home).ok()) else {
        return cwd.display().to_string();
    };
    if rest.as_os_str().is_empty() {
        return HOME_SYMBOL.to_owned();
    }
    Path::new(HOME_SYMBOL).join(rest).display().to_string()
}
