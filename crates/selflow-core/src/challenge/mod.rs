mod constants;
mod types;

use jiff::Timestamp;

use crate::enforce::{ActiveLock, GrantedExtension};

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
        reason: reason.to_owned(),
    })
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
