# Selflow M2.1 leftovers

- **Source:** `docs/superpowers/plans/2026-11-20-selflow-m2-1-rules-and-steps.md`. The items come
  from the run ledger of that plan (`progress.md`, kept in a local, git-ignored folder under
  `.superpowers/sdd/` that is deleted after this file is written) and from the final review's
  triage and the final fix wave reports in the same folder.
- **Created:** 2026-11-28.

M2.1 shipped with a list of minor findings that the reviews deferred, two items the controller
parked, and a few findings from the isolated test harness. This file keeps every one that is still
open at `b3bd491`, so the next plans (lock screen themes, M2b, M2.2, the IPC plan) can pick them up
instead of rediscovering them. Each item says what the problem is, where it is, and which M2.1 task
it came from. Items are grouped by area, and inside a group the ones that change what a person
sees come before the cosmetic ones. The final review's triage is marked where it gave one: it put
the unbounded `length` and the exception during an extension under "must fix" (both done), the
per-request parsing, the per-tick clones, the supervisor margins and the untested shutdown rules
under "should fix soon", and everything else under "can wait". The final fix wave then sent the
per-request parsing, the per-tick clones and the choice of M1 text to the M2b plan. The last
section lists what was dropped because it is fixed, with the fix.

The final review also set aside nine behaviors as outside M2.1: behavior inherited from M1, the
tamper resistance limits of main design section 8.7, designs the plan approved, and memory too small
to matter. The M2.1 run accepted them as out of scope, so they are not open work, and the list of the
nine wasn't kept in any file.

Line numbers are from `b3bd491` and drift; the function names are the stable reference.

## Daemon engine and notices

Behavior a person sees:

- **E1. A daemon restart within `WARNING_GRACE` (120 s) re-sends recent notices.** `sent` and
  `announced` live in memory (`crates/selflowd/src/engine/mod.rs`, fields at :36-37, `send_notices`
  at :135-178), so a restarted daemon sends again every notice from the last two minutes. Same
  cause as M1's notices. From Task 6 (minor), Task 7 (minor) and the isolation harness.
  **Covered by the IPC plan (`docs/superpowers/plans/2026-11-26-selflow-ipc-versioning.md`,
  Task 5).** That task stores the notices sent and the announcements per boot, writes them before
  the notice goes out, and restores them in `Engine::new` (tests `a_restart_does_not_repeat_a_notice`
  and `a_restart_keeps_the_announced_shutdown_time`). The state is per boot, so a reboot that
  comes back within `WARNING_GRACE` is not covered. Re-run the harness repro after that plan lands.
- **E2. A done `once` shutdown can announce "shuts down in a minute" again, with no shutdown
  after it.** `notices_for` (`crates/selflow-core/src/enforce/mod.rs:199`) keeps listing a done
  non-repeating window's notices until `ends_at`; a daemon back within `WARNING_GRACE` re-sends
  them, and with `announced` empty the notice is the false "shuts down in a minute". Task 7
  suggested dropping the notices of a done non-repeating window. From Task 7 (two minors) and the
  isolation harness. **Covered by the IPC plan (`docs/superpowers/plans/2026-11-26-selflow-ipc-versioning.md`,
  Task 5)**: the sent notices, the announcements and the rules that shut down are restored per boot,
  so the notice is not sent again. The cause in `notices_for` stays, so check the harness repro once
  that plan is done and drop this item only if it passes.
- **E3. A shutdown notice sent under a lock names `max(start, now + 1 min)`.** The real time is
  decided by the lock screen and can be a minute later, and no ruling covers the text of such a
  notice. Such notices do not count as announcements, which `a_shutdown_notice_under_a_lock_does_not_count_as_its_announcement`
  pins. `send_notices` and `as_told` in `crates/selflowd/src/engine/mod.rs`. From the final fix
  wave (shutdown notice time, commit 92572e2).
- **E4. A lock screen that shows a due rule counts an earlier desktop notice as that rule's
  announcement.** Implementer's note, Task 7 fix round. `announced_shutdown` and `shown_shutdown`
  in `crates/selflowd/src/engine/mod.rs`.
- **E5. The in-run `shut_down` filter covers only the earliest due window.** `tick` filters
  `evaluation.shutdown` against `self.shut_down` (`engine/mod.rs:62-66`); a later due window stays
  masked while the first stays due (LogOnly, or a failed poweroff). M1 had the same once-per-run
  behavior. From Task 7.
- **E6. In LogOnly mode only, a rule does not shut down again under a lock that already shut down
  in that run.** From Task 7.
- **E7. An unreadable `rule_shutdown` event detail fails the whole tick.** `shutdowns_on` in
  `crates/selflowd/src/store/mod.rs:380-397` returns the parse error, as unreadable
  `started_windows` rows do. M1's ruling R29 logs and retries instead. From Task 7.

Cleanup:

- **E8. `send_notices` rebuilds `windows_around` and matches by `starts_at == notice.moment`**
  (`engine/mod.rs:168-176`). That is a second source of truth next to `evaluate`. From Task 6.
- **E9. `announced_shutdown` reads like a getter but sends a notice and mutates `announced`**
  (`engine/mod.rs:299-322`). Commit e86b5ed renamed it from `announced_at`, but it still sends and
  mutates. From Task 7.
- **E10. The engine clones every cached `Config` into a new `History` each tick (about once a
  minute).** `history_with` and `config_of` (`engine/mod.rs:427-455`); the parse is cached in
  `self.parsed`, the clone is not. An `Arc` would avoid it. From Task 5. Final review triage:
  "should fix soon"; the fix wave sent it to the M2b plan.
- **E11. `engine/mod.rs` is 587 lines.** The shutdown-rule pieces could move to a sibling file.
  From Task 7.
- **E12. The engine `Record` enum carries `#[expect(clippy::large_enum_variant)]`**
  (`crates/selflowd/src/engine/types.rs:34`). Boxing `RuleWindow` would remove it. From Task 6.
- **E13. `Store::record_started` does a SELECT and then an UPDATE without a transaction**
  (`crates/selflowd/src/store/mod.rs:216-245`). The store sits behind the engine's mutex and no
  real race was found. From Task 6.

## IPC, handlers and exceptions

Behavior a person sees:

- **I1. The exception handlers take the started window's challenges but not its action**
  (`skippable` and `with_started_challenges`, `crates/selflowd/src/ipc/handlers.rs:296-329`).
  A lock removal saved 24 h ahead that lands between a night's first notice and its start makes
  that still-locking window skippable at once through the notice-only path. It needs a deliberate
  removal timed into a gap of about 15 minutes a day ahead, which is itself a waited loosening.
  Parked in the final review. Fix with the M2b plan by judging `enforces()` on the merged window.
- **I2. `locked_by` sees only the single active lock** (first by start). Since the
  final fix wave an exception for rule X is refused while X's lock or one of X's extensions runs,
  but an exception for rule B while rule A's lock is on screen is still allowed. It is never
  cheaper than an extension. `locked_by` in `handlers.rs:331`. From Task 9. The final review put
  the same-rule case under "must fix" (done) and left this part as a minor.
- **I3. The previous logical date's lock blocks a same-day exception.** A lock from 06:00 to
  07:00 stops an exception for tonight's window until it ends. This is conservative. `locked_by`,
  `handlers.rs:331`. From Task 9.

Cleanup and hardening:

- **I4. The handlers parse every stored version on every request, under the store mutex**
  (`stored_history` in `crates/selflowd/src/engine/mod.rs:423` builds a fresh parse cache per
  call; used at `handlers.rs:78, 131, 155, 482`). It grows with the number of saves. Note for the
  section 15 measurement. From Task 5. Final review triage: "should fix soon"; the fix wave sent it
  to the M2b plan.
- **I5. `release_screen` reads `views`, then calls `release()`** (`handlers.rs:162-167`). A lock
  shown in between gets its client killed once. Check `views` under the `running` lock inside
  `release`. From Task 8.
- **I6. `Store::restart_exception` has no `AND granted_at IS NULL` guard**
  (`crates/selflowd/src/store/mod.rs:275-286`). It is unreachable for granted rows today because
  of `skippable`. From Task 9.
- **I7. `extensions_left` repeats the count and saturating subtract of core's `active_lock`**
  (`handlers.rs:359`, `crates/selflow-core/src/enforce/mod.rs:157-180`). The count could live in
  core. From Task 9.
- **I8. `handlers.rs` is 639 lines.** The exception handlers could move to their own file. From
  Task 9.
- **I9. The daemon's subscriber-close probe no longer sees bytes buffered in `FramedRead`.** It
  reads one byte straight from the stream (`crates/selflowd/src/ipc/mod.rs:48-57`). From the
  LinesCodec side task (commit 6749ee0, review minor). The IPC plan replaces the codec but does
  not mention the probe, so check it there.
- **I10. LinesCodec side task review minors: a spurious `Ok(None)` after a codec error before the
  reader resumes, and no test for a line of exactly 1 MiB or for invalid UTF-8.** Commit 6749ee0.
  The IPC plan, Task 1, replaces the JSON line codec with `postcard` in length-delimited frames,
  which makes the UTF-8 case moot and tests a frame over the limit. Re-check the first and the
  exact-limit case against the new codec once Task 1 lands, and drop what no longer applies.

## Config, versions and save answers

Behavior a person sees:

- **C1. `report-config-saved` counts units, not changes.** One bedtime edit over five weekdays
  reads "5", and the harness saw 21 for 3 listed changes. The count is
  `json!({ "now": applied.len(), "waiting": later.len() })` in the `SaveConfig` handler
  (`handlers.rs:145`) rendered by `report-config-saved` (`locales/en/report.ftl:16`). From Task 5
  and the isolation harness.
- **C2. Challenge changes are listed for rules that cannot use them.** Reminders (neutral, but
  still listed) and lockless shutdowns (judged as a 24 h loosening) add noise to the save answer,
  `config status` and the report. `rule_fields` and `challenge_fields` in
  `crates/selflow-core/src/versions/compare.rs:117, 289`. From the isolation harness.
- **C3. A rule that comes back after its removal took effect, with the same loosened value that
  was still waiting, gets a `change-canceled` line in the save answer while the value applies at
  once with the rule's return.** Wording only, not a bypass. `plan_save` in
  `crates/selflow-core/src/versions/clock.rs`. From Task 4.
- **C4. The owner noted that a label-only re-save is not reported** while checking the save and
  status date in the Task 10 checklist (step 6). The ledger gives no more detail. `rule_fields`
  does emit a neutral `change-label` entry, so find out what the owner saw (the status date, or
  the save answer) before acting on it. From Task 10.
- **C5. While nothing is saved yet, closing `config edit` saves the default file**, with its
  23:00 to 07:00 lock and `shutdown_after = "30m"`, even unchanged. That is the intended effect
  of the first-save change (commit 7fd7ed5), but it surprises someone who opens the editor only
  to read. Open it outside those hours or set the times before closing. Note for the M2b editor.
  From the final review (note for the owner's checklist).
- **C6. `config edit` shows the latest M1 text by id** (`Store::m1_text`, `ORDER BY id DESC`,
  `crates/selflowd/src/store/mod.rs:189`; `config_text`, `handlers.rs:98`). Which M1 text should
  be shown is undecided. Final review item M3, sent to the M2b plan.

Validation and cleanup:

- **C7. `lock = true` without an end can add a spurious `config-locks-overlap`.** The rule gets
  a day-end window that counts as a lock (`window` at `validate.rs:284`, `locks(raw)` at :598),
  while `config-lock-needs-end` is also reported. From Task 3.
- **C8. An invalid `shutdown_after` on a rule without a lock reports two errors at one value**
  (a bad duration, then `config-shutdown-after-needs-lock`; `action` and `without_lock`,
  `validate.rs:369-405`). From Task 3.
- **C9. `crates/selflow-core/src/config/validate.rs` is 612 lines.** The exception and action
  validation could move to their own files. From Task 3.
- **C10. `assemble.rs:70` `unwrap_or(self.first)` is dead.** `source_at` always finds the first
  version saved by `at`, so the `first` field exists only for it. From Task 4.
- **C11. `still_waiting` (`clock.rs:203`), and so `waiting`, does not skip versions saved after
  `at`.** `config_at` does. Unreachable in production. From Task 4.
- **C12. The clock tests do not pin several behaviors.** Missing in `crates/selflow-core/tests/clocks.rs`
  (the weekday order of the rule sets from `config_at`, the set names and rule order, exceptions
  through `plan_save` and `config_at`, and the arguments of the setting and exception
  `*-canceled` ids). From Task 4.
- **C13. The M2.1 plan text still describes `render_saved` before the `cli-saved` change.**
  `docs/superpowers/plans/2026-11-20-selflow-m2-1-rules-and-steps.md:735-737` says it prints
  `cli-saved-now` first; the code prints `cli-saved` always and `cli-saved-now` only as the heading
  of changes that apply at once. Doc only. From Task 5.

## Lock screen and supervisor

- **L1. A release client that a lock starts under stays marked `releasing` for the whole lock.**
  `start()` (`crates/selflowd/src/enforcer/lock_supervisor.rs:167`) returns early when a client
  runs, and `spawn` sets `running.releasing` (:204). If that client freezes at the lock's end, no
  release client follows, and its death logs as a release exit. Clear `releasing` in `start()` when
  a view is shown and a client already runs. From Task 8.
- **L2. `hide()` while a release client already runs calls `stop_after_grace` again on every free
  tick** (`lock_supervisor.rs:66-75`). That is one short timer task per tick for at most 5 s and
  is harmless because of the generation check. From Task 8.
- **L3. `selflow-ui` compiles only on Linux until another `backend/<system>.rs` exists**
  (`crates/selflow-ui/src/lock/backend/mod.rs`, `Cargo.toml` target table). Spec section 19 adds
  systems that way. From Task 2.
- **L4. `crates/selflow-ui/src/app.rs:23-27` uses `if cli.lock {..} else {preview}`.** A `match`
  on both flags would be explicit; the no-flag case is unreachable under `arg_required_else_help`.
  From Task 2.

## CLI and copy

Behavior a person sees:

- **K1. `ipc-exception-locked` says the lock is on screen and to run `selflow unlock`, which is
  false during an extension,** where `selflow unlock` answers `ipc-not-locked`. Enforcement is
  right; only the copy misleads in that state. `locales/{en,pt-BR}/ipc.ftl:10`. Parked in the
  final review: reword it in the next change to that file or with M2b, for example "The { $label }
  lock is running tonight, so it can't be skipped."
- **K2. After an unlock grants an extension, `selflow test-lock` still says "A lock is on screen
  right now"** until the test window ends (`ipc-test-lock-busy`, `locales/en/ipc.ftl:5`; the
  `open_test` check in `test_lock`, `handlers.rs:384-415`). From the isolation harness.
- **K3. A reminder without a label uses its id in the notice title.** The label defaults to the
  id (`validate.rs:237`). From the isolation harness.
- **K4. Ctrl+D at the prompt gives poor output.** End of input after `ReasonTooShort` or
  `PhraseMismatch` prints the refusal twice (`answer_challenges` in
  `crates/selflow/src/prompt/mod.rs:128-141`, then `unlock_cmd.rs`). End of input before any answer
  prints `cli-protocol-error`, which blames the daemon (`input_ended`, `prompt/mod.rs:190`). From
  Task 8.
- **K5. The `[0]` variant of `notice-reminder-body-3` ("A good moment for a short pause.")
  assumes what a reminder is for.** `locales/{en,pt-BR}/warnings.ftl:44-48`. From Task 6.
- **K6. `lock-failed` is no longer referenced, and `selflow-ui --preview` failures show an English
  tracing line.** The id sits in `locales/{en,pt-BR}/lock.ftl:21`, and the error is logged in
  `crates/selflow-ui/src/app.rs:33`. The final review was to decide: remove the id or print the
  failure localized. From Task 2.
- **K7. A socket bind I/O failure surfaces as `AppError::Io` ("system error") instead of
  `AppError::Ipc` ("socket error").** `crates/selflowd/src/app.rs:44-47`. From Task 1.

Cleanup:

- **K8. Duplicated helpers.** `refusal_text` and `clock_time` in `crates/selflow/src/prompt/mod.rs:143-162`
  repeat `crates/selflow-ui/src/lock/view.rs:142-165`; `test_lock_cmd.rs:36` formats `HH:MM`
  inline; `unlock_cmd.rs:35` repeats `config_cmd.rs:27` for the language fallback. From Task 8.
- **K9. `crates/selflow/src/editor/mod.rs:6-8` puts `use crate::os::run_editor;` above the
  external imports.** Inconsistent ordering. From Task 1.
- **K10. `crates/selflow/Cargo.toml` keeps an unused `serde_json`, and `tokio`, whose only user
  now is `tests/support/mod.rs`.** `serde_json` was already unused in M1; `tokio` could become a
  dev-dependency. From Task 2.

## Tests

- **T1. `selflow-client` has no direct test of `exchange` or of the timeout path** (covered
  indirectly; `crates/selflow-client/tests/client.rs`). From Task 2.
- **T2. `tests/unlock.rs` does not cover `TooEarly`, `unlock_with`'s refusal exit code 1, or end
  of input before any answer** (`crates/selflow/tests/unlock.rs`). From Task 8.
- **T3. A `[[exception]]` in the file that covers the rule today is untested.** That is the
  `ipc-exception-done` case without a stored grant; the existing tests
  (`crates/selflowd/tests/ipc.rs:811, 898`) use a stored grant. From Task 9.
- **T4. The `cli-exception-no-budget` path is untested in `crates/selflow/tests/exception.rs`**
  (the local shortcut, and the daemon's `NoBudget` after the wait). From Task 9.
- **T5. `a_custom_message_picks_one_text_per_day` does not show that the date takes part in the
  draw** (`crates/selflow-core/tests/enforce.rs:166`; it only shows the choice is stable within a
  day). From Task 6.
- **T6. `after_an_upgrade_the_first_edit_shows_the_old_settings` does not pin the blank line, the
  `# config-before-update` note, or the bare `#` for empty lines** (`crates/selflowd/tests/ipc.rs:216`).
  From Task 5.
- **T7. `FakeDaemon` loops on `accept` forever, leaking a thread per test**
  (`crates/selflow/tests/support/mod.rs:153`). From Task 8.
- **T8. `crates/selflowd/tests/engine.rs:6` has a blank line splitting the `std` imports.** Import
  grouping nit. From Task 7.

## Already fixed

Each item below was in the ledger or the final review and is not carried above. The fix is named,
and the code at `b3bd491` was checked.

- **Task 3, a `length` of 24 h or more is not checked against the rule's own next-day copy
  (ledger line 92; final review C1, "must fix").** Fixed by b02ad6b `chore: reject rules that last
  a day or more` (`config-too-long` at the `length` value, `LONGEST_RULE`, rows in
  `reports_each_error_with_its_line`).
- **Task 9, challenge_view/answer_exception read the config in force, not a started window's
  merged challenges (ledger line 195).** Fixed by d56fc2e `chore: refuse an exception during the
  rule's own extension` (`with_started_challenges`, `Challenges::stricter`).
- **Task 9, `locked_by` ignores an exception for X during X's extension (the first half of ledger
  line 197; final review I1, "must fix").** Fixed by d56fc2e. The other half, an exception for B
  under A's lock, stays open as I2.
- **Task 7, untested brief rules of the lock-screen shutdown (ledger line 158; "should fix
  soon").** Fixed by c4135d0 `chore: harden the supervisor and shutdown tests` (four engine tests,
  each mutation-checked).
- **Task 8, the 11 s supervisor test and its fixed sleeps (ledger line 172; "should fix soon").**
  Fixed by c4135d0 (conditions replace the sleeps that guard an assertion).
- **Task 3, `windows_for`'s exception skip, non-lock filter and `After` end untested.** The
  function was rewritten in Task 6 (889a266 `chore: send notices for every rule`): the non-lock
  filter is gone, and `an_exception_day_skips_its_rules`
  (`crates/selflow-core/tests/enforce.rs:245`) and `windows_end_at_their_length_or_the_day_boundary`
  (`crates/selflow-core/tests/plan.rs:29`) cover the rest.
- **Task 3, `parse_offset` has no direct test of zero accepted or a negative offset rejected.**
  Covered at the configuration level: the `config-negative-offset` row in
  `reports_each_error_with_its_line` (8cb4358) and the `notices = ["0m"]` tests in
  `crates/selflow-core/tests/enforce.rs` (889a266).
- **Task 3, the "is a lock" check written four times.** 889a266 added `Action::locks` and
  `Rule::locks`, which core and `handlers.rs` use. Only the raw-config `locks(raw)` in
  `validate.rs` remains, on a different type.
- **Task 4, `waiting` still lists a change that will never apply when a rule returns while one of
  its own loosenings from before its removal waits.** Fixed by 3e03629 `chore: cancel a returning
  rule's waiting changes` (the plan_save change that cancels every waiting field change the file no
  longer asks for).
- **Task 4, tests' `looser` re-implements `clock.rs` `loosest`.** Fixed by fda7d72 `chore: store
  each change with the time it takes effect` (one public `stored_changes` in `selflow-core`, used
  by the handler, the daemon test support and core's `history_of`).
- **Task 8, a lock-client takeover needs Hyprland's `misc:allow_session_lock_restore = true`
  (carried to the README and the owner's checklist).** In the README (`README.md:22` and :69),
  committed in 56983f6 `docs: bring the README up to date with the current behaviour`.
- **Final review I2, I3 and the README minors (the `pgrep` guard that matched its own line, the
  daemon restart after `make install`, nothing runs until the first save, the status line,
  `test-lock --rule <set>/<id>`, "store over", the `config status` wording, a shutdown rule saved
  in its window).** README text in 56983f6 (`README.md:62-65` holds the bracketed `pgrep` guard);
  the code part of I3 (first save sent even when unchanged, new `cli-status-default`) is
  7fd7ed5 `chore: save the first configuration even when unchanged`.
- **Final review M2, a shutdown notice for an offset under a minute names the wrong time.** Fixed
  by 92572e2 `chore: name the real shutdown time in a shutdown notice`. The text of a notice sent
  under a lock stays open as E3.
