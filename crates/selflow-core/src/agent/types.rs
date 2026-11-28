use jiff::Timestamp;

use crate::config::AgentStep;
use crate::plan::RuleWindow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveAgentStep {
    pub key: String,
    pub window: RuleWindow,
    pub step: AgentStep,
    pub at: Timestamp,
    pub message: Option<String>,
}
