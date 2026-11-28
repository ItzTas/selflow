# Selflow phase 1 roadmap

- **Spec:** `docs/superpowers/specs/2026-11-17-selflow-design.md`
- **Date:** 2026-11-18. **Updated:** 2026-11-20 with the spec changes of that day: rules that
  need no lock, Claude Code steps inside rules, modes as defaults and limits, the terminal unlock,
  the split between `selflow` and `selflow-ui`, and the operating system layer. Later that day the
  M2 plan split M2 into M2.1 and M2.2, and the owner set the order after M1: M2.1, the lock screen
  themes, M2b, M2.2.

Phase 1 ("Stop", spec §17) is too big for a single plan, so it is split into milestones. Each
milestone ends with something the owner uses every day, and each gets its own detailed plan. The
next plan is written when the previous milestone is done, so it can build on the real code instead
of guessed interfaces; the plans for M1 and M2.1 are written. The order follows the spec: a
working night lock on one machine first, then the server and the multi-device pieces.

| Milestone | What the owner gets | Detailed plan |
|---|---|---|
| M1. Night lock | Bedtime warnings, a real lock screen with extensions, shutdown, and config changes that wait 24 hours to loosen. One machine, no server. | `2026-11-18-selflow-m1-night-lock.md` |
| M2.1. Rules and steps | Rules with any mix of notices, a lock and a shutdown (with or without a lock, once or on every boot), custom messages, challenges and a budget per rule, each change on its own clock, exception days, the terminal unlock, and the command line split from the graphical binary. Still one machine. | `2026-11-20-selflow-m2-1-rules-and-steps.md` |
| Lock screen themes | A lock screen as configurable as hyprlock's, from `~/.config/selflow/lock.toml`, that can't hide the challenge form or a coming shutdown. Still one machine. | not written yet |
| M2b. Claude Code sessions | Claude steps in any rule: messages to open Claude Code sessions, with a level that narrows what the agent takes on, reminders on every prompt, and edits and shell commands denied at the firmest level. Still one machine. | not written yet |
| M2.2. Watching apps | Rules about windows and processes, the local workday, app blocking, inhibit, and the tray and the small window. Still one machine. | not written yet |
| M3. Server and accounts | The server with accounts and devices, onboarding with modes, the configuration kept on the server, sync with an offline queue. | not written yet |
| M4. Deploy on the Pi | The server running on the Pi behind HTTPS, used by the laptop and the work PC. | not written yet |
| M5. One user, many devices | Locks and blocks that follow the user across machines, the shared budget, the cross-device workday and the morning report. | not written yet |

## Where each phase 1 item lands

| Spec §17 item | Milestone |
|---|---|
| Server with accounts, device tokens, closed registration and the admin CLI | M3 |
| Daemon and lock screen | M1 |
| Tray and the small window | M2.2 |
| `selflow` without a GUI library, and `selflow-ui` for the window, tray and lock screen | M2.1 (the split and the lock screen), M2.2 (the tray and the window) |
| Hyprland adapter | M2.2 |
| i18n and the systemd units | M1 |
| Terminal onboarding with modes as defaults and limits | M3 |
| Text configuration with `selflow config edit` and `selflow config status` | M1 (local), M3 (on the server) |
| Configuration versions and the loosening rules | M1 (local), M2.1 (each change on its own clock), M3 (on the server) |
| Exception days | M2.1 |
| Rules by time and anchor, reusable rule sets | M1 |
| Rules by window, process and local workday | M2.2 |
| Rules with any mix of steps: no lock needed, a shutdown with no lock, custom messages | M2.1 |
| App blocking and bounded inhibit | M2.2 |
| Terminal unlock (`selflow unlock`) | M2.1 |
| Unlock challenges (wait, reason, phrase, budget) and escalation up to shutdown | M1 |
| PIN held by someone else | M3 |
| Enforcement per user across devices, cross-device workday with the schedule fallback | M5 |
| Next-day report | M1 (`selflow report`, local), M5 (built by the server, morning notification) |
| Heartbeats and gaps in the report | M5 |
| Deployment on the Pi behind HTTPS | M4 |
| Claude Code steps in rules (§8.12) | M2b |

## M1. Night lock

Plan: `2026-11-18-selflow-m1-night-lock.md`.

Done when the owner uses it at night on the laptop: warnings at 15, 5 and 1 minute, a lock on every
monitor, one extension behind a 2-minute wait and a written reason, shutdown 30 minutes into the
lock, and `selflow config edit` showing that a later bedtime only takes effect 24 hours later. The
idle daemon stays under 30 MB of RAM and under 1 s of CPU per 10 minutes.

## M2.1 Rules and steps

Plan: `2026-11-20-selflow-m2-1-rules-and-steps.md`. M2 grew to about twenty tasks on 2026-11-20,
so its plan split it in two (Claude's call, approved by the owner): this first half already gives
the owner something to use every day, and M2.2 builds on the rule model it creates.

- First, the operating system layer, with no change in behavior: `selflow-platform` for paths,
  local connections and private files, `src/os/` in the daemon and the command line, Linux-only
  dependencies under `cfg` tables, and a Windows target in `make check` for `selflow-core`,
  `selflow-i18n` and `selflow-protocol` (§6.4, §19). The existing tests show that nothing
  changed, and the rule joins the layout line in `CLAUDE.md`.
- The split between the command line and the graphical binary: `selflow` links no GUI library,
  `selflow-ui` holds the lock screen with the session lock in its own backend file, and the client
  code both use lives in the new `selflow-client` crate (§6.1, §19, §20). The daemon starts the
  lock screen from `selflow-ui`.
- Rules with any mix of steps: notices, the lock, and a shutdown with or without a lock, always
  with a notice at least a minute before it, plus custom messages with defaults in both languages
  (§8.3). A shutdown without a lock happens once or repeats until the rule ends, as the rule says.
  The configuration format changes to hold them; the store starts over from M1's versions, keeps
  them aside, and shows them as comments in the first `selflow config edit` (§8.3).
- Challenges and an extension budget for each rule, with rule-set defaults the rule inherits
  (§8.5). M1 counts extensions per rule set and day.
- Each change in a save on its own clock: stricter and neutral changes at once, loosening ones
  after the delay, and a later save never restarts a wait (§8.9). M1 applies a whole save at the
  time of its loosest change, and every new save restarts the wait.
- Exception days: in the configuration at least a day ahead, or through the rule's challenges on
  the day itself (§8.9).
- `selflow unlock`: the terminal unlock, with the same challenges and budget as the screen, and the
  release of a lock stuck on a dead or frozen client (§8.13).

Done when the bedtime lock works from the new file through `selflow-ui`, a rule with only a notice
fires with its own text and no lock, a shutdown rule without a lock gives its minute of notice and
repeats after a restart only when it says so, an earlier bedtime saved together with an extra
extension applies at once while the extension waits on its own clock, the screen unlocks from a
TTY through the challenges, a same-day exception skips tonight's lock, `selflow` builds without
iced, `selflow-core`, `selflow-i18n` and `selflow-protocol` build for a Windows target, and the
daemon stays within budget.

## Lock screen themes

Added on 2026-11-20 (§8.5.1). A small milestone right after M2.1, because it needs only
`selflow-ui`.

- The theme file `~/.config/selflow/lock.toml`, outside the configuration versions: a theme change
  skips the loosening delay, and the next lock uses the saved file. `selflow-platform`'s paths
  gain the config folder (`XDG_CONFIG_HOME`).
- Widgets drawn with iced: a background (color, image, blur), labels, images, shapes, a countdown
  and the challenge form, each with a position, an alignment and an optional monitor, plus text
  variables such as `$label` and `$ends_at`.
- The challenge form and the shutdown time always show and stay legible: in their default place
  when the theme leaves them out, and always inside the screen, at a minimum size, on a backing
  plate with enough contrast.
- No commands, and only calm animations.
- Today's screen as the default theme, and `selflow lock theme init`, which writes a copy to start
  from.

Done when the owner's own `lock.toml` changes the next lock screen, and a theme that leaves out or
shrinks the challenge form or the shutdown time still shows them legibly.

## M2b. Claude Code sessions

Added on 2026-11-18 and reworked on 2026-11-20. It comes after M2.1 and the lock screen themes, in
the owner's order, and uses M2.1's rule steps. Its steps cover sessions by folder (§8.12), so it
doesn't need M2.2's work classification. It needs nothing from the server either, so it runs on one
machine like M1 and M2.1.

- Agent steps go through adapters, one per agent tool, with Claude Code as the first (§8.12, owner,
  2026-11-27). The plan shapes the hooks, the session registry and the delivery so that another
  agent, such as opencode, can be added later without touching the rules.
- First, an experiment against a live session to find the message framing on the inbox socket
  (§8.12, §21). Its result decides whether messages wake idle sessions or wait
  for the next prompt.
- Claude steps in rules: a time placed like any other step, a message with defaults in both
  languages, a level, and optional folders, all covered by the loosening rule (§8.3, §8.9, §8.12).
- Steps cover sessions someone is typing in. Runs nobody is watching carry on unless a step turns
  them on, and the report lists the ones that ran during a level. First check how a hook tells the
  two apart (`CLAUDE_CODE_SESSION_ATTENDED`, §8.12, §21).
- `selflow claude hook session-start|session-end|prompt|tool` and the daemon's session registry,
  which keeps the session id, the folder and the socket path, never the messaging token (§8.12).
- Delivery of the step messages over the inbox socket (§8.12).
- Behavior per level: the message only, reminders on every prompt from "Light tasks only" on, and
  at "Save only" the `PreToolUse` denial with the allowed commands. When several levels cover a
  session, the firmest wins (§8.12).
- `selflow claude install` and `selflow claude uninstall`, which edit `~/.claude/settings.json`,
  show the change before writing it, check the Claude Code version, and warn when
  `crossSessionInbound` or a default that bypasses permission prompts would keep messages from
  arriving (§8.12).
- Events with the rule, the step, the level and the folder, never transcripts or prompts, listed in
  `selflow report` (§8.12).

Done when, with a Claude Code session open in `~/Workspace/github.com/ItzTas/seconv`, the agent
gets the wind-down message at its time and passes it on, a prompt sent after it carries the
reminder, a "Save only" step denies an edit while `git commit` still works, a session that no
step covers gets nothing, and each `selflow claude hook` call finishes within 10 ms at the 95th
percentile on the owner's laptop, measured with the daemon running (§15).

## M2.2 Watching apps

The second half of M2, planned once M2b is built, on the code then in place.

- Hyprland adapter: the event socket, open windows and closing a window (§6.4, §15).
- Process detector reading `/proc` every 30 to 60 s, only when a rule uses processes (§15).
- Rules by window class or title and by process, timed "after X minutes", continuous or added up,
  with the notices before them predicted from the current activity (§8.1, §8.2, §8.3).
- Work classification and the local workday total (§8.1, §6.3).
- App blocking and the unblock flow, through the block rule's own challenges and budget (§8.6).
- Bounded inhibit, drawn from the budget of the lock it delays (§8.4).
- The tray (`selflow-ui --tray` with `ksni`, used directly or through `tray-icon`, §20) and the
  small window for status and unblocking (§6.1), started from the `hyprland.start` hook.
- Events keep the app class and the rule, never window titles (§6.7).

Done when a blocked app closes as soon as it opens and unblocks through the challenges, work after
hours triggers its rule, the tray shows the state, and the daemon stays within budget.

## M3. Server and accounts

- `selflow-server` with axum on the owner's PostgreSQL (its own database and role), migrations in
  `sql/schema/`, and `compose.yaml` for development (§6.6, §14, §19).
- Accounts (Argon2id, closed registration, `selflow-server user add`, rate limiting), device
  tokens and revocation (§7).
- Configuration versions on the server, with the server applying the loosening rule change by
  change (§8.9).
- `selflow init` in the terminal: server address, login, device registration, language, mode, work
  hours, bedtime and apps (§8.10).
- The Light, Balanced and Strict modes: each sets the defaults and how far the user can loosen, and
  a configuration that goes past its mode's limits is refused when saved (§8.10).
- The PIN held by someone else (§8.5).
- Daemon sync: configuration pull, event upload with the offline queue, and the push channel
  (§6.2, §6.6).

Done when the owner logs in from the laptop, edits the configuration through the server, and the
daemon keeps enforcing while the server is down.

## M4. Deploy on the Pi

- Check the Pi's architecture and operating system. The owner loads the Pi's SSH key first
  (`ssh-add ~/.ssh/keys/id_pi_home_1`).
- Cross-compile `selflow-server` for that architecture.
- A database and a role on the shared PostgreSQL, and the server as a systemd service on the Pi,
  listening on localhost.
- HTTPS through a reverse proxy, and how the Pi is reached from the internet, decided with the
  owner at this point.
- Privileged steps through `ssudo -H pi`.
- Measure the server on the Pi (§15).

Done when the laptop and the work PC use the server on the Pi over HTTPS.

## M5. One user, many devices

- Enforcement follows the user: locks and blocks reach the user's other devices within seconds,
  one shared nightly budget, offline overspending reconciled (§6.5).
- The cross-device workday with the schedule fallback (§6.3).
- Heartbeats, with gaps listed in the report (§8.7).
- The next-day report built by the server, a morning notification, and `selflow report` reading
  from the server (§8.11).

Done when a lock on the laptop appears on the work PC within seconds, and the morning report covers
both machines.
