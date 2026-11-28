use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use selflow_i18n::language::Language;
use selflow_i18n::localizer::{Arg, Localizer};
use selflow_platform::paths::socket_path;
use serde_json::Value;

use crate::claude::{
    ClaudeError, DeliveryWarning, HookEdit, HookEnv, MIN_INBOX_VERSION, delivery_warnings,
    hook_env, hook_event, hook_with, install, parse_claude_version, parse_settings,
    read_settings_bytes, uninstall, write_settings,
};
use crate::cli::CliError;
use crate::os::{claude_settings_path, claude_version_output};
use crate::prompt::{Prompt, TerminalPrompt};

#[derive(Debug, Args)]
pub struct ClaudeArgs {
    #[command(subcommand)]
    pub action: ClaudeAction,
}

#[derive(Debug, Subcommand)]
pub enum ClaudeAction {
    #[command(disable_help_flag = true)]
    Hook {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    Install {
        #[arg(long)]
        yes: bool,
    },
    Uninstall {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsChange {
    Install { yes: bool },
    Uninstall { yes: bool },
}

pub fn claude_cmd(args: ClaudeArgs) -> Result<ExitCode, CliError> {
    let change = match args.action {
        ClaudeAction::Hook { args } => return Ok(hook(&args)),
        ClaudeAction::Install { yes } => SettingsChange::Install { yes },
        ClaudeAction::Uninstall { yes } => SettingsChange::Uninstall { yes },
    };
    let settings = claude_settings_path().ok_or(ClaudeError::NoSettingsPath)?;
    let program = std::env::current_exe().map_err(|_| ClaudeError::NoProgramPath)?;
    let program = program.to_str().ok_or(ClaudeError::NoProgramPath)?;
    let version = matches!(change, SettingsChange::Install { .. })
        .then(claude_version_output)
        .flatten();
    let localizer = Localizer::new(Language::from_env());
    claude_with(
        change,
        &settings,
        program,
        version.as_deref(),
        &localizer,
        &mut TerminalPrompt,
    )
}

fn hook(args: &[OsString]) -> ExitCode {
    let socket = socket_path();
    claude_hook_with(
        args,
        io::stdin(),
        &hook_env(),
        socket.as_deref(),
        &mut io::stdout(),
    )
}

pub fn claude_hook_with(
    args: &[OsString],
    input: impl Read + Send + 'static,
    env: &HookEnv,
    socket: Option<&Path>,
    out: &mut dyn Write,
) -> ExitCode {
    let output = hook_event(args).and_then(|event| hook_with(event, input, env, socket));
    if let Some(output) = output {
        let _ = writeln!(out, "{output}");
    }
    ExitCode::SUCCESS
}

pub fn claude_with(
    change: SettingsChange,
    settings_path: &Path,
    selflow: &str,
    version_output: Option<&str>,
    localizer: &Localizer,
    prompt: &mut dyn Prompt,
) -> Result<ExitCode, CliError> {
    let (yes, installing) = match change {
        SettingsChange::Install { yes } => (yes, true),
        SettingsChange::Uninstall { yes } => (yes, false),
    };
    let original = read_settings_bytes(settings_path)?;
    let mut settings = parse_settings(settings_path, original.as_deref())?;
    let edits = match installing {
        true => install(&mut settings, selflow),
        false => uninstall(&mut settings),
    };
    let path = [("path", Arg::Text(settings_path.display().to_string()))];
    let warnings = match installing {
        true => install_warnings(&settings, version_output, localizer),
        false => Vec::new(),
    };
    if edits.is_empty() {
        prompt.say(&localizer.text("cli-claude-unchanged", &path));
        warnings.iter().for_each(|warning| prompt.say(warning));
        return Ok(ExitCode::SUCCESS);
    }
    prompt.say(&localizer.text("cli-claude-changes", &path));
    edits.iter().for_each(|edit| prompt.say(&edit_line(edit)));
    warnings.iter().for_each(|warning| prompt.say(warning));
    if !yes && !confirmed(localizer, prompt) {
        prompt.say(&localizer.text("cli-claude-declined", &path));
        return Ok(ExitCode::from(1));
    }
    if read_settings_bytes(settings_path)? != original {
        prompt.say(&localizer.text("cli-claude-changed", &path));
        return Ok(ExitCode::from(1));
    }
    write_settings(settings_path, &settings)?;
    prompt.say(&localizer.text("cli-claude-written", &path));
    Ok(ExitCode::SUCCESS)
}

fn edit_line(edit: &HookEdit) -> String {
    match edit {
        HookEdit::Added { event, command } => format!("+ {event}: {command}"),
        HookEdit::Removed { event, command } => format!("- {event}: {command}"),
    }
}

fn confirmed(localizer: &Localizer, prompt: &mut dyn Prompt) -> bool {
    let Some(answer) = prompt.ask(&localizer.text("cli-claude-confirm", &[])) else {
        return false;
    };
    ["y", "yes", "s", "sim"].contains(&answer.trim().to_lowercase().as_str())
}

fn install_warnings(
    settings: &Value,
    version_output: Option<&str>,
    localizer: &Localizer,
) -> Vec<String> {
    let delivery = delivery_warnings(settings)
        .into_iter()
        .map(|warning| delivery_text(&warning, localizer));
    version_warning(version_output, localizer)
        .into_iter()
        .chain(delivery)
        .collect()
}

fn version_warning(output: Option<&str>, localizer: &Localizer) -> Option<String> {
    let min = ("min", Arg::Text(dotted(MIN_INBOX_VERSION)));
    let Some(found) = output.and_then(parse_claude_version) else {
        return Some(localizer.text("cli-claude-missing", &[min]));
    };
    if found >= MIN_INBOX_VERSION {
        return None;
    }
    let found = ("found", Arg::Text(dotted(found)));
    Some(localizer.text("cli-claude-too-old", &[found, min]))
}

fn dotted((major, minor, patch): (u32, u32, u32)) -> String {
    format!("{major}.{minor}.{patch}")
}

fn delivery_text(warning: &DeliveryWarning, localizer: &Localizer) -> String {
    match warning {
        DeliveryWarning::Held { value } => {
            localizer.text("cli-claude-held", &[("value", Arg::Text(value.clone()))])
        }
        DeliveryWarning::Bypassed => localizer.text("cli-claude-bypassed", &[]),
    }
}
