# Selflow phase 1 roadmap

- **Spec:** `docs/superpowers/specs/2026-11-17-selflow-design.md`
- **Date:** 2026-11-18

Phase 1 ("Stop", spec §17) is too big for a single plan, so it is split into five milestones. Each
milestone ends with something the owner uses every day, and each gets its own detailed plan. Only
M1's plan is written now. The next plan is written when the previous milestone is done, so it can
build on the real code instead of guessed interfaces. The order follows the spec: a working night
lock on one machine first, then the server and the multi-device pieces.

| Milestone | What the owner gets | Detailed plan |
|---|---|---|
| M1. Night lock | Bedtime warnings, a real lock screen with extensions, shutdown, and config changes that wait 24 hours to loosen. One machine, no server. | `2026-11-18-selflow-m1-night-lock.md` |
| M2. Watching apps | Rules about windows and processes, app blocking, inhibit, exception days, the tray and the small window. Still one machine. | not written yet |
| M3. Server and accounts | The server with accounts and devices, onboarding with modes, the configuration kept on the server, sync with an offline queue. | not written yet |
| M4. Deploy on the Pi | The server running on the Pi behind HTTPS, used by the laptop and the work PC. | not written yet |
| M5. One user, many devices | Locks and blocks that follow the user across machines, the shared budget, the cross-device workday and the morning report. | not written yet |

## Where each phase 1 item lands

| Spec §17 item | Milestone |
|---|---|
| Server with accounts, device tokens, closed registration and the admin CLI | M3 |
| Daemon and lock screen | M1 |
| Tray and the small `selflow` window | M2 |
| Hyprland adapter | M2 |
| i18n and the systemd units | M1 |
| Terminal onboarding with modes | M3 |
| Text configuration with `selflow config edit` and `selflow config status` | M1 (local), M3 (on the server) |
| Configuration versions and the loosening rules | M1 (local), M3 (on the server) |
| Exception days | M2 |
| Rules by time and anchor, reusable rule sets | M1 |
| Rules by window, process and local workday | M2 |
| App blocking and bounded inhibit | M2 |
| Unlock challenges (wait, reason, phrase, budget) and escalation up to shutdown | M1 |
| PIN held by someone else | M3 |
| Enforcement per user across devices, cross-device workday with the schedule fallback | M5 |
| Next-day report | M1 (`selflow report`, local), M5 (built by the server, morning notification) |
| Heartbeats and gaps in the report | M5 |
| Deployment on the Pi behind HTTPS | M4 |

## M1. Night lock

Plan: `2026-11-18-selflow-m1-night-lock.md`.

Done when the owner uses it at night on the laptop: warnings at 15, 5 and 1 minute, a lock on every
monitor, one extension behind a 2-minute wait and a written reason, shutdown 30 minutes into the
lock, and `selflow config edit` showing that a later bedtime only takes effect 24 hours later. The
idle daemon stays under 30 MB of RAM and under 1 s of CPU per 10 minutes.

## M2. Watching apps

- Hyprland adapter: the event socket, open windows and closing a window (§6.4, §15).
- Process detector reading `/proc` every 30 to 60 s, only when a rule uses processes (§15).
- Rules by window class or title and by process, timed "after X minutes", continuous or added up
  (§8.1, §8.2).
- Work classification and the local workday total (§8.1, §6.3).
- App blocking and the unblock flow (§8.6).
- The tray (`selflow --tray` with `ksni`) and the small `selflow` window for status and unblocking
  (§6.1), started from the `hyprland.start` hook.
- Bounded inhibit (§8.4) and exception days (§8.9).
- Events keep the app class and the rule, never window titles (§6.7).

Done when a blocked app closes as soon as it opens and unblocks through the challenges, work after
hours triggers its rule, the tray shows the state, and the daemon stays within budget.

## M3. Server and accounts

- `selflow-server` with axum on the owner's PostgreSQL (its own database and role), migrations in
  `sql/schema/`, and `compose.yaml` for development (§6.6, §14, §19).
- Accounts (Argon2id, closed registration, `selflow-server user add`, rate limiting), device
  tokens and revocation (§7).
- Configuration versions on the server, with the server applying the loosening rule (§8.9).
- `selflow init` in the terminal: server address, login, device registration, language, mode, work
  hours, bedtime and apps (§8.10).
- The Light, Balanced and Strict presets (§8.10).
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
