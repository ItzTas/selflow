use crate::config::Rule;

pub(super) type CopyField = fn(&mut Rule, &Rule);

pub(super) const RULE_FIELDS: [(&str, CopyField); 10] = [
    ("timing", |rule, from| {
        rule.start = from.start;
        rule.start_jitter = from.start_jitter;
        rule.end = from.end;
    }),
    ("action", |rule, from| rule.action = from.action),
    ("notices", |rule, from| {
        rule.notices.clone_from(&from.notices)
    }),
    ("message", |rule, from| {
        rule.message.clone_from(&from.message)
    }),
    ("label", |rule, from| rule.label.clone_from(&from.label)),
    ("wait", |rule, from| {
        rule.challenges.wait = from.challenges.wait
    }),
    ("reason", |rule, from| {
        rule.challenges.reason = from.challenges.reason;
    }),
    ("phrase", |rule, from| {
        rule.challenges.phrase.clone_from(&from.challenges.phrase);
    }),
    ("extensions", |rule, from| {
        rule.challenges.extensions = from.challenges.extensions;
    }),
    ("extension_length", |rule, from| {
        rule.challenges.extension_length = from.challenges.extension_length;
    }),
];
