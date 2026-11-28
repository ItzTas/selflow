change-rule-added = The rule `{ $rule }` now applies on { $days }.
change-rule-removed = The rule `{ $rule }` no longer applies on { $days }.
change-rule-starts-earlier = The rule `{ $rule }` starts earlier on { $days }: { $old } → { $new }.
change-rule-starts-later = The rule `{ $rule }` starts later on { $days }: { $old } → { $new }.
change-rule-ends-later = The rule `{ $rule }` ends later on { $days }: { $old } → { $new }.
change-rule-ends-earlier = The rule `{ $rule }` ends earlier on { $days }: { $old } → { $new }.
change-rule-timing = The times of the rule `{ $rule }` are written differently on { $days }.
change-canceled = A change that was waiting for `{ $rule }` on { $days } won't happen.
change-lock-added = On { $days }, the rule `{ $rule }` now locks the screen.
change-lock-removed = On { $days }, the rule `{ $rule }` no longer locks the screen.
change-shutdown-added = On { $days }, the computer shuts down when the lock of `{ $rule }` has lasted { $new }.
change-shutdown-removed = On { $days }, the lock of `{ $rule }` no longer shuts the computer down after { $old }.
change-shutdown-sooner = On { $days }, the lock of `{ $rule }` shuts the computer down sooner: { $old } → { $new }.
change-shutdown-later = On { $days }, the lock of `{ $rule }` shuts the computer down later: { $old } → { $new }.
change-shutdown-at-start-added = On { $days }, the rule `{ $rule }` now shuts the computer down when it starts.
change-shutdown-at-start-removed = On { $days }, the rule `{ $rule }` no longer shuts the computer down when it starts.
change-shutdown-repeats = On { $days }, the rule `{ $rule }` shuts the computer down again after each boot until it ends.
change-shutdown-once = On { $days }, the rule `{ $rule }` shuts the computer down once, and not again after a boot.
change-notices = The notices of the rule `{ $rule }` change on { $days }: { $old } → { $new }.
change-message = The message of the rule `{ $rule }` changes on { $days }.
change-label = The name shown for the rule `{ $rule }` changes on { $days }: { $old } → { $new }.
change-wait-longer = For the rule `{ $rule }`, the wait before unlocking gets longer on { $days }: { $old } → { $new }.
change-wait-shorter = For the rule `{ $rule }`, the wait before unlocking gets shorter on { $days }: { $old } → { $new }.
change-reason-required = For the rule `{ $rule }`, unlocking asks for a written reason on { $days }.
change-reason-dropped = For the rule `{ $rule }`, unlocking no longer asks for a written reason on { $days }.
change-phrase-added = For the rule `{ $rule }`, unlocking asks you to type a sentence on { $days }: { $new }
change-phrase-dropped = For the rule `{ $rule }`, unlocking no longer asks you to type a sentence on { $days }.
change-phrase-changed = For the rule `{ $rule }`, the sentence to type changes on { $days }: { $old } → { $new }
change-fewer-extensions = For the rule `{ $rule }`, each day allows fewer extensions on { $days }: { $old } → { $new }.
change-more-extensions = For the rule `{ $rule }`, each day allows more extensions on { $days }: { $old } → { $new }.
change-shorter-extensions = For the rule `{ $rule }`, each extension gets shorter on { $days }: { $old } → { $new }.
change-longer-extensions = For the rule `{ $rule }`, each extension gets longer on { $days }: { $old } → { $new }.
change-day-start = The day starts at a different time: { $old } → { $new }.
change-delay-longer = Changes that loosen the rules wait longer before they take effect: { $old } → { $new }.
change-delay-shorter = Changes that loosen the rules wait less before they take effect: { $old } → { $new }.
change-setting-canceled = A change that was waiting for `{ $setting }` won't happen.
change-language = The language changes: { $old } → { $new }.
change-exception-added =
    { $rules ->
        [all] On { $date }, no rule runs.
       *[other] On { $date }, these rules don't run: { $rules }.
    }
change-exception-removed =
    { $rules ->
        [all] On { $date }, every rule runs again.
       *[other] On { $date }, these rules run again: { $rules }.
    }
change-exception-wider =
    { $rules ->
        [all] The exception for { $date } now covers every rule.
       *[other] The exception for { $date } now covers more rules: { $rules }.
    }
change-exception-narrower = The exception for { $date } now covers fewer rules: { $rules }.
change-exception-changed = The exception for { $date } now covers other rules: { $rules }.
change-exception-canceled = A change that was waiting for the exception on { $date } won't happen.
change-agent-step-added = For the rule `{ $rule }`, a Claude Code step is added on { $days }: { $new }.
change-agent-step-removed = For the rule `{ $rule }`, a Claude Code step is removed on { $days }: { $old }.
change-agent-step-firmer = For the rule `{ $rule }`, a Claude Code step gets firmer on { $days }: { $old } → { $new }.
change-agent-step-looser = For the rule `{ $rule }`, a Claude Code step gets looser on { $days }: { $old } → { $new }.
change-agent-step-changed = For the rule `{ $rule }`, a Claude Code step changes on { $days }: { $old } → { $new }.
change-agent-message = For the rule `{ $rule }`, the message of Claude Code step ({ $step }) changes on { $days }.
change-agent-commands-fewer = At the `save-only` level, Claude Code sessions can run fewer commands: { $old } → { $new }.
change-agent-commands-more = At the `save-only` level, Claude Code sessions can run more commands: { $old } → { $new }.
change-agent-commands-changed = The commands Claude Code sessions can run at the `save-only` level change: { $old } → { $new }.
