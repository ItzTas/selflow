config-syntax = This part of the file can't be read: { $detail }
config-bad-duration = The value `{ $value }` isn't a duration like 15m or 2h 30m.
config-not-positive = The duration `{ $value }` needs to be longer than zero.
config-bad-time = The value `{ $value }` isn't a time like 23:00.
config-unknown-anchor = There's no anchor named `{ $name }` under [anchors].
config-unknown-language = The language `{ $value }` isn't available. The options are "en" and "pt-BR".
config-bad-day = The value `{ $value }` isn't a day. The days are mon, tue, wed, thu, fri, sat and sun.
config-day-in-two-sets = The day `{ $day }` is in both `{ $first }` and `{ $second }`. Each day belongs to one rule set.
config-duplicate-set = There's already a rule set named `{ $name }`.
config-duplicate-rule = The rule set `{ $set }` already has a rule with the id `{ $id }`.
config-empty-window = The rule `{ $id }` starts and ends at the same time.
config-jitter-too-long = The start_jitter of the rule `{ $id }` needs to be shorter than the rule itself.
config-too-long = The rule `{ $id }` needs to last less than a day, counting its length and any start_jitter.
config-locks-overlap = In the rule set `{ $set }`, the locks of the rules `{ $first }` and `{ $second }` overlap.
config-negative-offset = The time `{ $value }` can't be negative. Use 0m for right at the start.
config-end-and-length = The rule `{ $id }` has both end and length. Keep the one you mean.
config-lock-needs-end = The rule `{ $id }` locks the screen, so it needs an end or a length to say when the lock lifts.
config-shutdown-after-needs-lock = The rule `{ $id }` has shutdown_after but no lock. Add lock = true, or use shutdown to shut down at the start.
config-shutdown-with-lock = The rule `{ $id }` has both lock and shutdown. With a lock, use shutdown_after instead.
config-bad-shutdown = The value `{ $value }` isn't a way to shut down. The options are "once" and "repeat".
config-bad-message = The message of the rule `{ $id }` needs to be a text or a list of texts.
config-empty-message = The message of the rule `{ $id }` has an empty text.
config-bad-date = The value `{ $value }` isn't a date like 2026-12-25.
config-duplicate-exception = There's already an exception for { $date }.
config-exception-unknown-rule = The exception for { $date } names `{ $rule }`, but the rule set for that day has no rule with that id.
config-agent-bad-level = The level `{ $value }` in a Claude step of the rule `{ $id }` isn't one Selflow knows. The levels are message, light, no-new-work and save-only.
config-agent-bad-folder = The folder `{ $value }` in a Claude step of the rule `{ $id }` needs to be `~`, start with `~/`, or be an absolute path. None of its parts can be `.` or `..`.
config-agent-duplicate-step = The rule `{ $id }` has two Claude steps with before = `{ $before }` and level `{ $level }`. Remove one of them, or change its before or its level.
config-agent-too-early = A Claude step of the rule `{ $id }` has before = `{ $value }`. It needs to be under 24h.
config-agent-whole-minutes = A Claude step of the rule `{ $id }` has before = `{ $value }`. Use whole minutes, like 1m or 90m.
config-agent-empty-command = An entry of allowed_commands under [agent] is empty. Write a command like "git status", or remove the entry.
config-before-update = Your settings from before this Selflow update, kept here for reference. Selflow no longer reads them, so copy what you want to keep into the file above.
