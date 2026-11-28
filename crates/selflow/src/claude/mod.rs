mod constants;
mod hook;
mod settings;
mod types;

pub use constants::{HOOK_DEADLINE, MIN_INBOX_VERSION};
pub use hook::{attended, hook_env, hook_output, hook_request, hook_with};
pub use settings::{
    delivery_warnings, install, parse_claude_version, read_settings, uninstall, write_settings,
};
pub use types::{ClaudeError, DeliveryWarning, HookEdit, HookEnv, HookEvent};

use constants::TOOL_MATCHER;

impl HookEvent {
    pub fn settings_event(self) -> &'static str {
        match self {
            HookEvent::SessionStart => "SessionStart",
            HookEvent::SessionEnd => "SessionEnd",
            HookEvent::Prompt => "UserPromptSubmit",
            HookEvent::Tool => "PreToolUse",
        }
    }

    pub fn arg(self) -> &'static str {
        match self {
            HookEvent::SessionStart => "session-start",
            HookEvent::SessionEnd => "session-end",
            HookEvent::Prompt => "prompt",
            HookEvent::Tool => "tool",
        }
    }

    pub fn matcher(self) -> Option<&'static str> {
        (self == HookEvent::Tool).then_some(TOOL_MATCHER)
    }
}
