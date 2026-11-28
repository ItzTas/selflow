use std::io;
use std::path::PathBuf;

use clap::ValueEnum;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    Prompt,
    Tool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookEnv {
    pub inbox: Option<String>,
    pub attended: Option<String>,
    pub entrypoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookEdit {
    Added {
        event: &'static str,
        command: String,
    },
    Removed {
        event: String,
        command: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryWarning {
    Held { value: String },
    Bypassed,
}

#[derive(Debug, Error)]
pub enum ClaudeError {
    #[error("no Claude Code settings path is known")]
    NoSettingsPath,
    #[error("the selflow program path is unknown")]
    NoProgramPath,
    #[error("{path} isn't valid JSON: {detail}")]
    Invalid { path: PathBuf, detail: String },
    #[error("{path} couldn't be read or written: {source}")]
    File {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
