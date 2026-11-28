use selflow_core::agent::ActiveAgentStep;
use selflow_core::config::DEFAULT_ALLOWED_COMMANDS;
use selflow_i18n::language::Language;
use serde::Serialize;
use thiserror::Error;

use crate::store::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directives {
    pub steps: Vec<ActiveAgentStep>,
    pub allowed_commands: Vec<String>,
    pub language: Language,
}

impl Default for Directives {
    fn default() -> Directives {
        Directives {
            steps: Vec::new(),
            allowed_commands: DEFAULT_ALLOWED_COMMANDS
                .iter()
                .map(|command| (*command).to_owned())
                .collect(),
            language: Language::En,
        }
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
}

#[derive(Serialize)]
pub(super) struct InboxFrame<'a> {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) message: InboxMessage<'a>,
}

#[derive(Serialize)]
pub(super) struct InboxMessage<'a> {
    pub(super) role: &'static str,
    pub(super) content: &'a str,
}
