mod constants;
mod error;
mod raw;
mod types;
mod validate;

pub(crate) use constants::DAYS;
pub use constants::SUPPORTED_LANGUAGES;
pub use error::ConfigError;
pub use types::{Challenges, Config, LockRule, RuleSet};

use raw::RawConfig;

pub fn parse(text: &str) -> Result<Config, Vec<ConfigError>> {
    let raw: RawConfig =
        toml::from_str(text).map_err(|error| vec![ConfigError::syntax(text, &error)])?;
    validate::validate(&raw, text)
}
