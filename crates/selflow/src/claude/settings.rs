use std::ffi::OsStr;
use std::fs;
use std::io::{self, ErrorKind, Write};
use std::path::Path;

use serde_json::{Map, Value, json};
use tempfile::NamedTempFile;

use super::constants::{
    BYPASS_MODE, HOOK_EVENTS, HOOK_TIMEOUT_SECONDS, INBOUND_ACCEPT, INBOUND_KEY, PROGRAM_NAME,
};
use super::types::{ClaudeError, DeliveryWarning, HookEdit, HookEvent};

pub fn read_settings(path: &Path) -> Result<Value, ClaudeError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(source) => return Err(file_error(path, source)),
    };
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str::<Map<String, Value>>(&text)
        .map(Value::Object)
        .map_err(|error| ClaudeError::Invalid {
            path: path.to_owned(),
            detail: error.to_string(),
        })
}

pub fn write_settings(path: &Path, settings: &Value) -> Result<(), ClaudeError> {
    let failed = |source: io::Error| file_error(path, source);
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    let folder = target
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(folder).map_err(failed)?;
    let mut text = serde_json::to_string_pretty(settings).map_err(|error| failed(error.into()))?;
    text.push('\n');
    let mut temp = NamedTempFile::new_in(folder).map_err(failed)?;
    temp.write_all(text.as_bytes()).map_err(failed)?;
    if let Ok(metadata) = fs::metadata(&target) {
        temp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(failed)?;
    }
    temp.as_file().sync_all().map_err(failed)?;
    temp.persist(&target).map_err(|error| failed(error.error))?;
    Ok(())
}

fn file_error(path: &Path, source: io::Error) -> ClaudeError {
    ClaudeError::File {
        path: path.to_owned(),
        source,
    }
}

pub fn install(settings: &mut Value, selflow: &str) -> Vec<HookEdit> {
    let Some(hooks) = settings.as_object_mut().and_then(|root| {
        root.entry("hooks")
            .or_insert_with(|| json!({}))
            .as_object_mut()
    }) else {
        return Vec::new();
    };
    HOOK_EVENTS
        .into_iter()
        .flat_map(|event| install_event(hooks, event, selflow))
        .collect()
}

fn install_event(hooks: &mut Map<String, Value>, event: HookEvent, selflow: &str) -> Vec<HookEdit> {
    let name = event.settings_event();
    let Some(groups) = hooks
        .entry(name)
        .or_insert_with(|| json!([]))
        .as_array_mut()
    else {
        return Vec::new();
    };
    let wanted = selflow_command(selflow, event);
    let mut current = false;
    let removed = remove_selflow_entries(groups, event, |command| {
        let kept = !current && command == wanted;
        current |= kept;
        kept
    });
    let mut edits = removed_edits(name, removed);
    if current {
        return edits;
    }
    groups.push(selflow_group(event, &wanted));
    edits.push(HookEdit::Added {
        event: name,
        command: wanted,
    });
    edits
}

pub fn uninstall(settings: &mut Value) -> Vec<HookEdit> {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Vec::new();
    };
    let edits: Vec<HookEdit> = HOOK_EVENTS
        .into_iter()
        .flat_map(|event| uninstall_event(hooks, event))
        .collect();
    let emptied = !edits.is_empty() && hooks.is_empty();
    if let Some(root) = settings.as_object_mut().filter(|_| emptied) {
        root.shift_remove("hooks");
    }
    edits
}

fn uninstall_event(hooks: &mut Map<String, Value>, event: HookEvent) -> Vec<HookEdit> {
    let name = event.settings_event();
    let Some(groups) = hooks.get_mut(name).and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    let removed = remove_selflow_entries(groups, event, |_| false);
    if !removed.is_empty() && groups.is_empty() {
        hooks.shift_remove(name);
    }
    removed_edits(name, removed)
}

fn removed_edits(name: &str, removed: Vec<String>) -> Vec<HookEdit> {
    removed
        .into_iter()
        .map(|command| HookEdit::Removed {
            event: name.to_owned(),
            command,
        })
        .collect()
}

fn remove_selflow_entries(
    groups: &mut Vec<Value>,
    event: HookEvent,
    mut keep: impl FnMut(&str) -> bool,
) -> Vec<String> {
    let mut removed = Vec::new();
    groups.retain_mut(|group| {
        let Some(entries) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = entries.len();
        entries.retain(|entry| {
            let Some(command) = entry["command"].as_str() else {
                return true;
            };
            if !is_selflow_command(command, event) || keep(command) {
                return true;
            }
            removed.push(command.to_owned());
            false
        });
        entries.len() == before || !entries.is_empty()
    });
    removed
}

fn selflow_command(selflow: &str, event: HookEvent) -> String {
    format!("{} claude hook {}", quoted(selflow), event.arg())
}

fn quoted(program: &str) -> String {
    if !program.contains(char::is_whitespace) {
        return program.to_owned();
    }
    format!("'{}'", program.replace('\'', r"'\''"))
}

fn selflow_group(event: HookEvent, command: &str) -> Value {
    let hooks = json!([{"type": "command", "command": command, "timeout": HOOK_TIMEOUT_SECONDS}]);
    match event.matcher() {
        Some(matcher) => json!({"matcher": matcher, "hooks": hooks}),
        None => json!({"hooks": hooks}),
    }
}

fn is_selflow_command(command: &str, event: HookEvent) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let Some((program, tail)) = words.len().checked_sub(3).map(|at| words.split_at(at)) else {
        return false;
    };
    if program.is_empty() || tail != ["claude", "hook", event.arg()] {
        return false;
    }
    let program = program.join(" ");
    let program = program
        .strip_prefix('\'')
        .and_then(|inner| inner.strip_suffix('\''))
        .unwrap_or(&program);
    Path::new(program).file_name() == Some(OsStr::new(PROGRAM_NAME))
}

pub fn delivery_warnings(settings: &Value) -> Vec<DeliveryWarning> {
    let inbound = settings.get(INBOUND_KEY).filter(|value| !value.is_null());
    let accepted = inbound.and_then(Value::as_str) == Some(INBOUND_ACCEPT);
    let mut warnings = Vec::new();
    if let Some(value) = inbound.filter(|_| !accepted) {
        let value = value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_owned);
        warnings.push(DeliveryWarning::Held { value });
    }
    let bypassed = settings["permissions"]["defaultMode"].as_str() == Some(BYPASS_MODE);
    if bypassed && !accepted {
        warnings.push(DeliveryWarning::Bypassed);
    }
    warnings
}

pub fn parse_claude_version(output: &str) -> Option<(u32, u32, u32)> {
    let mut parts = output.split_whitespace().next()?.split('.').map(str::parse);
    let version = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    parts.next().is_none().then_some(version)
}
