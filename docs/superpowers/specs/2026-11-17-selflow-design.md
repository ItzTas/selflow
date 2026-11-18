# Selflow: design

- **Status:** approved by the owner on 2026-11-18
- **Created:** 2026-11-17. **Updated:** 2026-11-18 with the answers to the builder check and the
  database choice.
- **Name:** Selflow. Binaries: `selflow`, `selflowd`, `selflow-server`.

Decisions marked "Claude's call" were settled by Claude while writing this document and are open
to the owner's veto. Everything else was decided by the owner during the design conversation.

## 1. Why this exists

Selflow is for people with ADHD, autism, or both (AuDHD). Many ADHD apps don't fit what these
users need, and few of them run on Linux. Three problems come up again and again:

1. Not stopping. Work spills into personal time: the workday ends on one machine and carries on at
   home on another, with no break and sometimes no lunch.
2. Not starting. Someone wants to do something and can't begin. For people who take medication,
   the problem often comes back in the evening, when the medication wears off.
3. Hyperfocus that crowds out everything else. Hours go into one thing while sleep and other basic
   needs get neglected. The same happens on the phone, with series, video after video, or
   short-video feeds.

The autistic side and the ADHD side want opposite things. The autistic side needs predictability,
a warning before each transition, and few surprises. The ADHD side needs flexibility and novelty.
It doesn't feel time passing, it tunes out anything repetitive, and it drops a whole routine the
first time the routine breaks.

Selflow is a guardian of transitions rather than a to-do app. It makes time visible and enforces
the moments when the user should stop, start or switch. The guiding principle is a predictable
structure on the outside with flexibility inside it.

Selflow starts as a personal tool, but one server can host several people, each with their own
devices, rules and data.

## 2. Goals

- Stop the "work continues on the laptop" pattern by counting one workday across all of a user's
  machines.
- Make stopping at night actually happen, escalating from warnings to a screen lock and, if the
  lock is ignored, a shutdown.
- Support a routine that serves the autistic side and the ADHD side at once (section 9).
- Shape the day around the user's medication schedule (section 10).
- Let users configure everything: rules, timings, challenges, intensity, and each routine behavior.
- Let each user pick how strict Selflow is during onboarding (section 8.10).
- Stay light enough for a laptop with 8 GB of RAM and a Raspberry Pi server (section 15).
- Run self-hosted, with several users on one server.

## 3. Non-goals

- Selflow gives no medical advice. Onset, peak and rebound times are whatever the user enters.
  There is no built-in drug database and the app never suggests values.
- There is no phone client in phases 1 to 6. iOS limits what an app can control, so a phone client
  needs its own design work. The design leaves room for it in phase 7 and doesn't build it earlier.
- Only Hyprland is supported in phases 1 to 6. Both of the owner's machines run it. Other
  environments come later as adapters (section 6.4).
- Selflow doesn't try to stop a determined user with root access. Enforcement adds enough friction
  to outlast an impulse (section 8.7).

## 4. Anti-goals

These would count as failure even if the app technically works:

- A watcher that costs noticeable CPU, RAM or battery.
- An easy bypass: a PIN the user knows by heart, an inhibit window that cancels the lock just by
  being open, or a config change that weakens a rule right away (section 8.9).
- Punishment or shame: streaks to lose, red alerts, guilt-laden wording.
- Loud sensory input. Interruptions are firm but calm, without flashing or loud sounds.
- A rigid schedule that marks the day as failed when it breaks, instead of re-planning it.
- A plan where nothing is usable until everything is done. Every phase ends with something the
  owner uses daily.

## 5. Constraints

- Rust for the server, the daemon and the UI.
- Linux (Arch) with Hyprland on both the work PC and the personal laptop.
- English everywhere: code, identifiers, UI and docs. Internationalization from day one, with
  English and Brazilian Portuguese (pt-BR) both complete in every phase.
- Several users per server. Each user has their own devices, configuration, enforcement state and
  data, and never sees another user's data (section 7).
- Medication data is encrypted at rest (section 10.2).
- The server API must not assume the client is a desktop, so a phone can join later as one more
  client.
- The server runs on the owner's personal Raspberry Pi and is reachable from the internet over
  HTTPS (section 14).

## 6. Architecture

### 6.1 Components

| Component | Role |
|---|---|
| `selflow-server` | Self-hosted server, backed by PostgreSQL. It stores accounts and devices, each user's configuration (with versions, section 8.9), the activity and event log, workday totals, the unlock and extension log, and the next-day reports. It holds each user's live enforcement state and pushes it to their devices. From phase 4 it also sends email. |
| `selflowd` | The daemon. One per machine, run as a systemd user service with `Restart=always`. It watches windows, processes and time, evaluates rules, triggers warnings, locks, app blocks and shutdowns, and talks to the server. It keeps a local SQLite store (section 6.7) and doesn't link any GUI library. |
| `selflow --tray` | A small process that holds the tray icon (StatusNotifierItem, through `ksni`) and talks to the daemon over a local socket. Hyprland starts it at login from the `hyprland.start` hook of the owner's Lua config (Hyprland 0.57). |
| `selflow` | The command line and the window. The CLI covers onboarding (`selflow init`), configuration (`selflow config edit`, `selflow config status`) and reports (`selflow report`). In phase 1 the window is small: it shows what is active and handles unblocking (section 8.6, Claude's call). The full GUI editor comes in phase 6. Only one window runs at a time, and calling `selflow` again brings the open one to the front. |
| Lock screen | Part of the `selflow` binary, started by the daemon. An iced UI running as a real Wayland session lock (section 8.5). |

Killing the tray icon doesn't stop enforcement, because the daemon is a separate process. The
daemon is also a separate binary, so it never links the GUI library. The owner approved this split
together with the repository layout (section 19).

### 6.2 Offline behavior

The daemon keeps a local copy of its user's effective configuration in its SQLite store
(section 6.7) and keeps enforcing when the server can't be reached. Time-based rules fire on every
device from that copy. Events wait in a local queue until the server is back. Two things degrade
while offline: the workday time from the user's other devices (the daemon uses the last known
total plus local activity) and the shared extension budget (section 6.5). (Claude's call)

### 6.3 Cross-device workday

Rules classify activity as work (section 8.1). Each daemon reports work time to the server, and
the server keeps the day's running total across all of the user's devices. When a daemon starts,
or when a work window becomes active, it fetches the current total. Opening the laptop at 19:00
after eight hours on the work PC counts as the same workday.

If no device reported work during the user's configured work hours, the server assumes those hours
were worked. This covers a work PC that can't reach the server. (Claude's call)

### 6.4 Environment adapters

Everything that depends on the desktop environment sits behind one adapter interface: reading the
active and open windows, closing windows, and locking the screen. Phases 1 to 6 ship only the
Hyprland adapter. GNOME, KDE, X11 or other systems can be added later as new adapters without
touching the rules engine.

### 6.5 Enforcement follows the user (Claude's call)

Moving to another machine is the easiest way around a lock, and work that carries on from one
machine to another is one of the problems in section 1. Enforcement therefore follows the user
instead of the machine.

- A lock or app block that starts on one device applies to all of that user's online devices
  within a few seconds.
- The extension budget (section 8.5) is one budget per user per night, shared by all their
  devices. Inhibits draw from it too.
- An offline device can spend at most the budget it knew about at its last sync. If two offline
  devices overspend, the server reconciles the log once they reconnect, and the next-day report
  shows the overspend.

### 6.6 Server and transport

- `axum`, with PostgreSQL as the server database. Selflow uses its own database and role on the
  owner's PostgreSQL instance, which other services share (section 14).
- The server pushes enforcement changes (a lock started, an extension granted, a new config
  version) to the user's daemons within a few seconds, as section 6.5 requires. The builder picks
  the mechanism (WebSocket, long polling or similar), as long as an idle connection stays within
  the resource budget (section 15).

### 6.7 Local storage on each device

The owner chose SQLite for the local store on each user's machine. It is a single file used from
inside the daemon, with no extra process, which fits the resource budget and works offline.

- The file lives in `$XDG_STATE_HOME/selflow/` (by default `~/.local/state/selflow/`). Only the
  user can read it: the directory has mode 0700 and the file mode 0600.
- It holds what enforcement and sync need: the effective configuration and the versions still
  waiting, the offline event queue, local activity counters, and the last known extension budget.
  Events are deleted once the server confirms it received them.
- Events record the app class, the rule that matched and the durations. They don't record window
  titles, which can contain private text such as document names or browser tab titles. Rules can
  still match on titles when they are evaluated. (Claude's call, approved by the owner. It applies
  to the server's event log too.)
- The local store is not encrypted in phase 1. The owner wants to look at encryption more calmly
  later, and phase 3 is the planned moment, when medication data starts to be cached on the
  device. The likely approach is SQLCipher, with the key kept in the system keyring. `rusqlite`
  0.43 has SQLCipher features, and SQLCipher 4.20.0 came out in October 2026 from an active
  repository. Phase 3 has to define what happens when the keyring is locked at startup, so that
  enforcement never stops because the key isn't available. Until then, full-disk encryption is
  what protects a lost laptop.

## 7. Users and security

The server is open to the internet, so it needs real accounts and protection against password
guessing. The owner asked for several users and some form of security. The specific measures in
this section are Claude's call.

### 7.1 Accounts

- Each user logs in with a username and password. The server stores passwords hashed with
  Argon2id.
- Registration is closed. The admin creates users from the server's command line (for example
  `selflow-server user add`), so a stranger who finds the server can't sign up. The first user
  created at install time is the admin.
- Failed logins are rate limited, with a growing delay per account and per address.
- Two-factor login (TOTP) is optional and arrives in phase 4.

### 7.2 Devices

- A device logs in once and receives its own token. The password is never stored on the device.
- Users can list their devices and revoke any of them. A revoked token stops working immediately.

### 7.3 Isolation between users

- Every stored record belongs to exactly one user, and every API call is scoped to the user who
  made it.
- Selflow has no screen or command that lets the admin read another user's data. Someone with
  shell access to the Pi can still read the database. Section 10.2 explains what that means for
  medication data.

### 7.4 Connection security

- A reverse proxy on the Pi terminates HTTPS, and `selflow-server` listens only on localhost
  behind it.
- Daemons refuse to talk to the server over plain HTTP, except to localhost during development.

## 8. Rules engine

### 8.1 Conditions

A rule can watch:

- the active window and the open windows, by class or title;
- running processes;
- time of day, day of week, and time windows;
- anchors, which are named times such as `bedtime` or `lunch`, usable anywhere a time is expected
  (section 9);
- the user's workday total across devices;
- from phase 3, medication state: taken or not, time since the dose, and the current phase
  (onset, peak, rebound);
- from phase 4, the URL or site open in the browser, through a browser extension;
- from phase 4, plugins that add new condition sources ("detectors") without changes to the core.

Rules can also classify activity. For example, "VS Code or Slack open after 18:00 counts as work".
The workday total is built from that classification.

### 8.2 Timing

- After a condition has been true for X minutes, either continuously or added up over the day.
  The rule says which.
- At a time, with an optional random window: "between 22:00 and 22:30" instead of "at 22:00". The
  bounds are predictable, which the autistic side needs. The exact minute and the wording of the
  message change from day to day, so the ADHD side doesn't learn to ignore them.

### 8.3 Actions

- Show a desktop notification or warning.
- Lock the screen for X time, until a given time, or for the rest of a time window.
- Shut the machine down after a lock has been ignored or bypassed for a configured time.
- Block an app (section 8.6).
- Inhibit a lock (section 8.4).
- From phase 4, send an email.

The default escalation goes from warnings to a lock to a shutdown, and each step is configurable.
Countdown warnings come before the lock and before the shutdown, so nothing is cut off without
notice.

### 8.4 Inhibit

An inhibit only delays a lock and never cancels it. Its limits:

- It applies only to conditions the user configured on purpose, such as a video call app.
- Each inhibit has a maximum delay.
- Each inhibit counts against the night's extension budget.
- Every inhibit is logged and shows up in the next-day report.

### 8.5 Lock screen and unlock challenges

The lock is a real Wayland session lock (`ext-session-lock`), so Alt+Tab or a workspace switch
can't get around it. The screen is calm and shows why it is locked, how long remains, and which
challenges are available.

The challenge types, configured per rule set:

1. A mandatory wait, for example two minutes before the unlock option appears.
2. A written reason, which is logged and shown in the next-day report.
3. A long sentence the user has to type.
4. An extension budget: at most N extensions per night, each with a fixed length. When the budget
   is gone, nothing unlocks the screen.
5. Optionally, a PIN that someone else holds.

The default is a wait, a written reason and an extension budget. A PIN or security question the
user knows by heart is never a default, because it doesn't interrupt an impulse.

### 8.6 Blocking apps

The owner asked for a rule that keeps closing an app every time it opens, until they unblock it
through Selflow. The details below are Claude's call.

- When a window matching a block rule opens, the daemon closes it. The rule can instead terminate
  the app's process, because some apps keep running in the tray after their window closes.
- Hyprland reports new windows as events, so a block rule costs nothing while nothing opens.
- After closing the app, Selflow shows a notification saying the app is blocked. Clicking it, or
  opening `selflow`, starts the unblock flow.
- Unblocking goes through the same challenges as the lock screen, lasts a fixed configured time,
  and counts against the extension budget.

### 8.7 Tamper resistance

- The daemon is a systemd user service with `Restart=always`, so killing it brings it back.
- The daemon sends heartbeats. The server records the gaps, and the next-day report lists them.
- A user can still disable the service. The goal is that a bypass takes more effort than an
  impulse is willing to spend.

### 8.8 Reusable rule sets

Rules are grouped into named rule sets, such as "Weeknight" or "Friday night", which are assigned
to days or day patterns. Editing a rule set changes it everywhere it is used.

### 8.9 Changing the configuration

Approved by the owner. This rule keeps "everything configurable" from turning into a bypass.

- A change that makes Selflow stricter takes effect right away.
- A change that loosens it takes effect 24 hours after it is saved, and it never changes a
  warning, lock or app block that has already started. Loosening means anything that weakens
  enforcement: a later time, a shorter lock, a disabled or deleted rule, more or longer
  extensions, a shorter wait, fewer challenges, more inhibit, a higher workday limit, an anchor
  moved past its allowed shift (section 9), or a lighter mode (section 8.10). When the app can't
  tell whether a change loosens or tightens, it treats it as loosening.
- The 24-hour delay is configurable, and shortening it counts as loosening.
- The server stores the configuration as versions, each with the time it takes effect.
  `selflow config status` shows the changes still waiting, and the next-day report lists them.
- Exception days (a holiday, a sick day, an on-call night) pass without friction when scheduled at
  least 24 hours ahead. On the same day, they go through the unlock challenges and show up in the
  report.
- If a wrong rule locks the screen at a bad moment, such as during a meeting, the unlock
  challenges are the emergency exit.

### 8.10 Strictness modes and onboarding

The owner asked for a mode, lighter or stricter, chosen during onboarding. The three presets below
are Claude's call. A mode is a set of starting values. Every setting stays individually
configurable afterwards, and switching to a lighter mode counts as loosening (section 8.9).

| Mode | What it sets |
|---|---|
| Light | Warnings and locks with a short wait to unlock, a generous extension budget, no shutdown step, a short loosening delay |
| Balanced | The defaults described in this document: a wait, a written reason and a small extension budget, a shutdown after a long ignored lock, a 24-hour loosening delay |
| Strict | Longer waits, no extensions after bedtime, an earlier shutdown, a loosening delay longer than 24 hours |

The builder picks the exact numbers, and they get tuned during real use.

In phase 1, onboarding runs in the terminal as `selflow init`. It asks for the server address and
the login, then registers the device. For a user with no configuration yet, it also asks for the
language, the mode, work hours, bedtime and the main apps to watch. A graphical onboarding comes
with the editor in phase 6.

### 8.11 Editing rules and reading reports in phase 1

Approved by the owner.

- Each user's configuration is a human-editable text file, in the spirit of `hyprland.conf`.
- `selflow config edit` downloads the current version from the server, opens it in `$EDITOR`,
  validates it and uploads it. The server then applies section 8.9. Validation errors are clear
  and come in both languages.
- The next-day report arrives as a desktop notification in the morning, and `selflow report`
  shows the full report in the terminal.
- The graphical editor comes near the end of the plan, in phase 6.

## 9. AuDHD routine (phase 2)

Six behaviors, each one configurable and each one possible to switch off:

1. Fixed anchors and flexible blocks. A few fixed points in the day (lunch, end of work,
   medication, bedtime), with blocks between them whose order the user picks on the spot.
2. A warning before every transition. The default is 15, 5 and 1 minute before, and the steps are
   configurable.
3. Randomness inside known bounds: random windows and varied message wording (section 8.2).
4. Re-planning instead of failing. When the routine breaks, for example because an unexpected
   meeting eats lunch, the app shows an explicit new plan for the rest of the day. It never marks
   the day as failed.
5. A day shaped by the medication curve, finished in phase 3. Hard tasks go in the peak and light
   ones in the rebound. During the rebound, the app asks only for the smallest first step ("open
   the file") instead of the whole task.
6. No punishment and no noise. There are no streaks to lose and no red alerts. The next-day report
   says what happened without judging it, and the intensity of notifications is configurable.

How the routine works in practice (Claude's call):

- Selflow learns that the routine changed from the user, through buttons in the tray menu and in
  notifications ("lunch moved", "skipped"), and from what the daemon already knows, such as an
  extension used or a lock overridden. It doesn't infer changes from idle time or a calendar. A
  calendar detector could come with the phase 4 plugins.
- A new plan is proposed, and the user confirms it with one click. Applying it automatically is a
  setting.
- Anchors are named times that rules can use from phase 1, so a rule says "lock at bedtime"
  instead of "lock at 23:00". Each anchor says whether it can move and by how much on the same
  day. Moving it within that limit is a normal re-plan. Moving it further is loosening
  (section 8.9). Bedtime can't move by default.
- In phase 2 the `selflow` window gains a routine view with today's timeline and the re-plan
  confirmation. The routine itself is still configured in the text file. Its visual design is
  still open (section 21).

## 10. Medication (phase 3)

### 10.1 Features

- Register each medication: name, optional dose, scheduled times, and user-entered durations for
  onset, peak and rebound.
- A reminder to take it, a log of when it was actually taken, and an alert when a scheduled dose
  has no log after a configured time.
- The day's curve (onset, then peak, then rebound) feeds behavior 5 of section 9.
- Rules can use medication state (section 8.1). Examples: "during the rebound, suggest only light
  tasks", "if not taken by 10:00, remind me".
- No medical advice (section 3).

### 10.2 Encryption at rest (Claude's call)

- The server encrypts medication data with a key kept in a file outside the database, readable
  only by the server's system user on the Pi.
- This protects against a leaked database file or a leaked backup. It doesn't protect against
  someone with shell access to the Pi, and with several users the admin could technically decrypt
  other users' data.
- It is not end-to-end encryption, because the server has to read medication state to send
  reminders such as "not taken by 10:00" by email while every computer is off, and to put
  medication in the reports.
- The server starts unattended, without a passphrase at boot.
- The key needs its own backup (section 14). Losing it loses the medication history.

## 11. Reach (phase 4)

- Email reminders, sent by the server over SMTP, on a fixed schedule with an optional random
  window. Their main job is reaching the phone at night while there is no phone client.
- URL and site rules, through a browser extension that reports the active tab to the local daemon.
- Plugins, through a standard detector contract: a plugin watches something and reports it to the
  daemon. The builder picks the plugin mechanism (WASM, external processes or something else)
  within the resource budget.
- Optional two-factor login with TOTP (section 7.1).

## 12. Tasks and reminders (phase 5)

Configurable tasks and reminders, with times written in natural language in the style of Todoist
("tomorrow at 3pm", "every Monday"), in English and pt-BR. Tasks connect to the "smallest first
step" behavior in section 9. This phase is not the focus and comes late on purpose.

## 13. GUI editor (phase 6)

The owner wants the graphical editor near the end of the plan. It covers rules, rule sets, the
routine, medications, challenges and modes, along with a graphical onboarding and a report view.
The text file stays supported, and changes made in the editor follow the same loosening rules
(section 8.9).

## 14. Hosting

- The server runs on the owner's personal Raspberry Pi, reachable as `ssh pi`. It doesn't run on
  any work infrastructure.
- The Pi's architecture and operating system haven't been checked yet, because its SSH key needs a
  passphrase that wasn't loaded in the agent. Check them before the deploy work. The server binary
  is cross-compiled on the desktop for the Pi's architecture.
- A reverse proxy on the Pi terminates HTTPS (section 7.4). How the Pi is reached from the
  internet (port forwarding with dynamic DNS, a tunnel, or something else) is decided at deploy
  time.
- The Pi already runs a PostgreSQL instance shared by the owner's other services. Selflow gets one
  more database there, with its own role that can't reach the other databases.
- Backups on the Pi are already handled by the owner's Backrest setup. The only Selflow-specific
  item is the medication key file (section 10.2): its path has to be inside what Backrest backs
  up, because losing the key loses the medication history.
- Privileged steps on the Pi go through the owner's `ssudo` helper, which runs `sudo` over SSH
  with a graphical password prompt on the desktop and stores nothing. Always pass `-H pi`, because
  the helper's default host is a different machine.

## 15. Resource budget

Neither the daemon nor the server may weigh on its machine. The rules:

1. Window changes are event driven. Hyprland's event socket pushes them, and nothing polls.
2. Timers compute the next action and sleep until then. Nothing ticks every second.
3. Processes are polled rarely, every 30 to 60 seconds, and only if an active rule uses a process
   condition.
4. Sync with the server is batched, except for enforcement changes, which must reach the user's
   other devices within seconds (section 6.5).
5. The daemon has no GUI. `selflowd` doesn't link iced at all. iced runs only inside the `selflow`
   binary, for the window and the lock screen, and only while one of them is on screen.

The targets have to be measured, not assumed. The daemon should use close to 0% CPU when idle and
stay in the low tens of MB of RAM. The server process should run comfortably on the Pi next to the
owner's other services. Each phase counts as done only after measuring the daemon on the owner's
laptop and the server on the Pi.

## 16. Internationalization

- `fluent`, Mozilla's i18n system, which handles pt-BR plurals and gender.
- English is the source language. pt-BR is complete from day one, and no phase ships with
  untranslated strings.
- Translation files live in `locales/en/` and `locales/pt-BR/`, one `.ftl` file per area
  (`lock.ftl`, `rules.ftl` and so on).
- The terminal onboarding, the config validation errors and the emails are translated too.
- Phase 5's natural-language date parsing handles both languages.

## 17. Phases

Each phase ends with something used daily.

1. **Stop.** The largest phase. It covers:
   - the server with accounts, device tokens, closed registration and the admin CLI;
   - the daemon, the tray, the small `selflow` window and the lock screen;
   - the Hyprland adapter, i18n and the systemd units;
   - terminal onboarding with modes, and the text configuration with `selflow config edit` and
     `selflow config status`;
   - configuration versions, the loosening rules and exception days;
   - rules by window, process, time, anchor and workday, and reusable rule sets;
   - app blocking, unlock challenges, escalation up to shutdown, and bounded inhibit;
   - enforcement per user across devices, and the cross-device workday with its schedule fallback;
   - the next-day report;
   - deployment on the Pi behind HTTPS.

   Inside this phase, the plan should first reach a working night lock on one machine, then add
   the server and the multi-device pieces, so the owner has a usable tool early. (Claude's call)
2. **AuDHD routine.** Section 9, including the routine view.
3. **Medication.** Section 10, plus encryption of the local store (section 6.7).
4. **Reach.** Section 11.
5. **Tasks and reminders.** Section 12.
6. **GUI editor.** Section 13.
7. **Future.** A phone client (iPhone first) and other desktop environments.

## 18. Decided stack (status checked 2026-11-17, session lock rechecked 2026-11-18)

| Piece | Choice | Status |
|---|---|---|
| UI | `iced` 0.14 | Last release in December 2025, repository active |
| Session lock | `iced_exwlshell` 0.21.4, `sessionlock` module | Released 2026-11-06 by the same team, built on iced 0.14. It replaces `iced_sessionlock`, whose code has left the repository. Still a small project, which makes it a risk. Fallback: write the lock client directly on `smithay-client-toolkit` |
| Tray | `ksni` | Active, pure Rust, no GTK |
| Server | `axum` | Active |
| Server database | PostgreSQL | The owner's choice: the shared instance on the Pi |
| Local store | SQLite | The owner's choice. Encryption planned for phase 3 (section 6.7) |
| i18n | `fluent` | Repository active |

## 19. Repository layout

Approved by the owner. It follows the conventions of the owner's other Rust projects
(mime-guardian, diario-alerr).

```
selflow/
├── Cargo.toml              # workspace: members, shared dependencies and lints
├── Cargo.lock              # committed
├── .prototools             # Rust toolchain pinned with proto
├── Makefile                # check, test, run targets
├── compose.yaml            # development PostgreSQL only
├── README.md
├── CLAUDE.md               # layout and code conventions, so every session keeps them
├── crates/
│   ├── selflow-core/       # pure domain logic: rules, rule sets, routine, challenges, medication; no I/O
│   ├── selflow-protocol/   # message types shared by server, daemon, UI and a future phone client
│   ├── selflow-i18n/       # fluent loading, used by all three binaries
│   ├── selflow-server/     # binary `selflow-server`: server and admin CLI; sql/schema/ holds the PostgreSQL migrations
│   ├── selflowd/           # binary `selflowd`: the daemon
│   └── selflow/            # binary `selflow`: CLI, window, `--tray`, lock screen
├── locales/
│   ├── en/                 # one .ftl file per area
│   └── pt-BR/              # the same files, translated
├── contrib/                # systemd units, .desktop file, icon, Hyprland Lua snippet
├── extensions/browser/     # phase 4: browser extension for URL rules
├── packaging/aur/          # later: PKGBUILD
├── docs/superpowers/       # specs/ and plans/
├── .githooks/              # commit-msg (Conventional Commits), pre-commit (fmt, clippy, tests)
└── .github/workflows/      # CI
```

Dependency direction: `selflow-core` and `selflow-i18n` depend on no other workspace crate.
`selflow-protocol` may depend on `selflow-core`. The three binary crates depend on the libraries
and never on each other.

Daemon internals:

```
crates/selflowd/src/
├── main.rs
├── lib.rs
├── app.rs          # wiring
├── adapter/        # desktop environment interface; hyprland/ first
├── detector/       # window, process, time (phase 4: url, plugins)
├── engine/         # evaluates the core rules, schedules timers
├── enforcer/       # notifications, app blocking, spawning the lock screen, shutdown
├── store/          # local SQLite store: config cache, offline queue, counters
├── sync/           # server client, push channel, heartbeat
└── ipc/            # local socket for the tray and the window
```

Conventions inside every crate:

- `main.rs` contains only `main`. Wiring lives in `app.rs`.
- `lib.rs` only declares modules.
- One module per subject. When a module grows, it becomes a folder with `mod.rs`, `types.rs` and
  `constants.rs`.
- Each subject module owns its types and its own error enum.
- CLI commands live in `src/cli/commands/<name>_cmd.rs`, each with `<Name>Args` and `<name>_cmd()`.
- Integration tests live in `tests/`, with shared helpers in `tests/support/`, one file per
  subject.
- Database migrations live in `crates/selflow-server/sql/schema/`. A schema change is always a new
  migration, and existing migrations are never edited.
- A test that needs PostgreSQL prints a `skipping:` notice and returns when the database isn't
  available, and fails when `CI` is set.

## 20. Left to the builder

- The module layout inside `selflow-core`, `selflow-server` and `selflow`, following section 19.
- The daemon-to-server protocol and the push mechanism, within section 6.6.
- The database schemas, and the PostgreSQL driver, SQLite driver and migration tool, each checked
  on crates.io before it is added. The owner's diario-alerr uses `tokio-postgres` with `deadpool`.
- The details of the Hyprland IPC.
- The format of the configuration file, as long as it is human editable, allows comments and gives
  clear errors in both languages.
- The exact values of the strictness modes (section 8.10).
- The plugin mechanism (section 11).

## 21. Open questions

- The visual design of the routine view and of the `selflow` window. To settle before phase 2,
  ideally with mockups.
- The Pi's architecture and operating system, and how it is reached from the internet. To check
  before the deploy work in phase 1.
