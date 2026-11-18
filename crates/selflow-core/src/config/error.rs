use std::ops::Range;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{id} at line {line}, column {column}")]
pub struct ConfigError {
    pub id: &'static str,
    pub args: Vec<(&'static str, String)>,
    pub line: usize,
    pub column: usize,
}

impl ConfigError {
    pub(super) fn at(
        text: &str,
        span: Range<usize>,
        id: &'static str,
        args: Vec<(&'static str, String)>,
    ) -> ConfigError {
        let before = text.get(..span.start).unwrap_or(text);
        let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
        ConfigError {
            id,
            args,
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
        }
    }

    pub(super) fn syntax(text: &str, error: &toml::de::Error) -> ConfigError {
        let detail = vec![("detail", error.message().to_owned())];
        ConfigError::at(
            text,
            error.span().unwrap_or_default(),
            "config-syntax",
            detail,
        )
    }
}
