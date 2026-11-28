mod constants;
mod error;
mod raw;
mod types;
mod validate;

pub(crate) use constants::DAYS;
pub use constants::{AGENT_LEVELS, DEFAULT_ALLOWED_COMMANDS, SUPPORTED_LANGUAGES};
pub use error::ConfigError;
pub use types::{
    Action, AgentLevel, AgentStep, Challenges, Config, Exception, Rule, RuleEnd, RuleSet,
};

use jiff::civil::Weekday;
use raw::RawConfig;

pub(crate) fn day_tag(day: Weekday) -> &'static str {
    DAYS[day.to_monday_zero_offset() as usize].0
}

pub fn parse(text: &str) -> Result<Config, Vec<ConfigError>> {
    let raw: RawConfig =
        toml::from_str(text).map_err(|error| vec![ConfigError::syntax(text, &error)])?;
    validate::validate(&raw, text)
}
