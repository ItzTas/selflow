mod constants;
mod types;

use jiff::{Timestamp, civil};

use crate::config::{Challenges, Rule};
use crate::enforce::{ActiveLock, GrantedException, GrantedExtension};

pub use constants::MIN_REASON_CHARS;
pub use types::{ExtensionAnswer, Refusal};

pub fn request_extension(
    now: Timestamp,
    lock: &ActiveLock,
    answer: &ExtensionAnswer,
) -> Result<GrantedExtension, Refusal> {
    let challenges = &lock.window.challenges;
    if now < lock.extension_available_at {
        return Err(Refusal::TooEarly {
            available_at: lock.extension_available_at,
        });
    }
    if lock.extensions_left == 0 {
        return Err(Refusal::NoBudget);
    }
    let reason = answered(challenges, answer)?;
    let until = now
        .checked_add(challenges.extension_length)
        .unwrap_or(Timestamp::MAX)
        .min(lock.window.ends_at.timestamp());
    Ok(GrantedExtension {
        rule_set: lock.window.set.clone(),
        rule_id: lock.window.rule_id.clone(),
        logical_date: lock.window.logical_date,
        granted_at: now,
        until,
        reason,
    })
}

pub fn request_exception(
    now: Timestamp,
    requested_at: Timestamp,
    rule: &Rule,
    logical_date: civil::Date,
    extensions_left: u32,
    answer: &ExtensionAnswer,
) -> Result<GrantedException, Refusal> {
    let granted = |reason: String| GrantedException {
        rule_id: rule.id.clone(),
        logical_date,
        granted_at: now,
        reason,
    };
    if !rule.enforces() {
        return Ok(granted(answer.reason.trim().to_owned()));
    }
    let available_at = requested_at
        .checked_add(rule.challenges.wait)
        .unwrap_or(Timestamp::MAX);
    if now < available_at {
        return Err(Refusal::TooEarly { available_at });
    }
    if extensions_left == 0 {
        return Err(Refusal::NoBudget);
    }
    answered(&rule.challenges, answer).map(granted)
}

fn answered(challenges: &Challenges, answer: &ExtensionAnswer) -> Result<String, Refusal> {
    let reason = answer.reason.trim();
    if challenges.reason && reason.chars().count() < MIN_REASON_CHARS {
        return Err(Refusal::ReasonTooShort {
            min_chars: MIN_REASON_CHARS,
        });
    }
    if let Some(phrase) = &challenges.phrase
        && normalized(phrase) != normalized(&answer.phrase)
    {
        return Err(Refusal::PhraseMismatch);
    }
    Ok(reason.to_owned())
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
