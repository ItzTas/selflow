use std::time::Duration;

use super::types::HookEvent;

pub const HOOK_DEADLINE: Duration = Duration::from_secs(1);
pub const MIN_INBOX_VERSION: (u32, u32, u32) = (2, 1, 224);
pub(super) const HOOK_EVENTS: [HookEvent; 4] = [
    HookEvent::SessionStart,
    HookEvent::SessionEnd,
    HookEvent::Prompt,
    HookEvent::Tool,
];
pub(super) const HOOK_TIMEOUT_SECONDS: u64 = 5;
pub(super) const TOOL_MATCHER: &str = "Edit|Write|MultiEdit|NotebookEdit|Bash|PowerShell|Monitor";
pub(super) const EDIT_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];
pub(super) const PROGRAM_NAME: &str = "selflow";
pub(super) const PLAIN_PATH_CHARS: &str = "/._+@%,:=-";
pub(super) const MAX_LINKS: usize = 40;
pub(super) const INBOUND_KEY: &str = "crossSessionInbound";
pub(super) const INBOUND_ACCEPT: &str = "accept";
pub(super) const BYPASS_MODE: &str = "bypassPermissions";
