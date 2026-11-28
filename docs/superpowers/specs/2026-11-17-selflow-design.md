# Selflow: design

- **Status:** approved by the owner on 2026-11-18
- **Created:** 2026-11-17. **Updated:** 2026-11-18 with the answers to the builder check, the
  database choice and the Claude Code integration (section 8.12). **Updated:** 2026-11-20 with
  rules that need no lock (section 8.3), Claude Code steps inside rules (section 8.12), modes as
  defaults and limits (section 8.10), the terminal unlock (section 8.13), the split between the
  command line and the graphical binary (section 6.1), and the operating system layer
  (section 19). **Updated:** 2026-11-27 with the rule to keep only the data Selflow needs
  (section 2), agent adapters for agent steps (section 8.12), trusted contacts over WhatsApp in
  phase 4 (section 11), the full interface in phase 6 (section 13), and the designs still to come
  (section 21).
- **Name:** Selflow. Binaries: `selflow`, `selflowd`, `selflow-server`, and from M2 `selflow-ui`.

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
- Let users configure everything: rules, timings, challenges, intensity, messages, and each routine
  behavior.
- Let each user pick how strict Selflow is during onboarding (section 8.10).
- Stay light enough for a laptop with 8 GB of RAM and a Raspberry Pi server (section 15).
- Run self-hosted, with several users on one server.
- Keep only the data each feature really needs, on the devices and on the server (owner,
  2026-11-27). A feature that can work without storing something doesn't store it.

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
| `selflow` | The command line. From M2 it links no GUI library. It covers onboarding (`selflow init`), configuration (`selflow config edit`, `selflow config status`), reports (`selflow report`), the terminal unlock (`selflow unlock`, section 8.13) and the Claude Code hooks (section 8.12). While the daemon runs in test mode (`SELFLOW_POWER=log`, where a shutdown is only logged), `selflow test-lock` starts a short lock with a chosen rule, so the owner can check the lock screen without waiting for bedtime. |
| `selflow-ui` | The graphical binary, from M2. It holds the window, the tray and the lock screen, and it is the only binary that links iced. (The name is Claude's call.) |
| `selflow-ui --tray` | A small process that holds the tray icon (StatusNotifierItem, through `ksni`) and talks to the daemon over a local socket. Hyprland starts it at login from the `hyprland.start` hook of the owner's Lua config (Hyprland 0.57). |
| Window | `selflow-ui` started with no arguments. In phase 1 the window is small: it shows what is active and handles unblocking (section 8.6, Claude's call). The full interface comes in phase 6 (section 13). Only one window runs at a time, and starting `selflow-ui` again brings the open one to the front. |
| Lock screen | Part of `selflow-ui`, started by the daemon. An iced UI running as a real Wayland session lock (section 8.5). |

Killing the tray icon doesn't stop enforcement, because the daemon is a separate process. The
daemon is also a separate binary, so it never links the GUI library. The owner approved this split
together with the repository layout (section 19).

M1 ships the command line and the lock screen together in the `selflow` binary. The owner chose on
2026-11-20 to split them in M2, for two reasons Claude gave. The terminal unlock (section 8.13) is
the way out when the screen fails, so it shouldn't share a binary with the graphics stack that
failed. And the Claude Code hooks start the command line on every prompt and every tool call, so it
should stay small. The code both binaries use to talk to the daemon moves to a library crate,
because binaries never depend on each other (section 19).

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

Code that depends on the operating system rather than the desktop has a layer of its own
(section 19): where files live, the local connection to the daemon, files only the user can open,
and shutting the machine down. Another operating system, such as Windows, would come in the same
way, as new files beside the Linux ones.

### 6.5 Enforcement follows the user (Claude's call)

Moving to another machine is the easiest way around a lock, and work that carries on from one
machine to another is one of the problems in section 1. Enforcement therefore follows the user
instead of the machine.

- A lock or app block that starts on one device applies to all of that user's online devices
  within a few seconds.
- Each rule's extension budget for the day (section 8.5) is shared by all of the user's devices.
  An inhibit draws from the budget of the lock it delays.
- An offline device can spend at most the budget it knew about at its last sync. If two offline
  devices overspend, the server reconciles the log once they reconnect, and the next-day report
  shows the overspend.

Rules that belong to one device (owner, 2026-11-28). A rule can name the devices it applies to, for
example the laptop the owner works on. Its notices and locks happen only on those devices, and its
locks don't follow the user to the others, because the rule is about that machine: leaving the work
laptop for another machine is what such a rule asks for. A rule that names no device applies to all
of them and follows the user as above. Until the server holds the configuration (M3), each machine
keeps its own, so every rule already belongs to the machine it was saved on.

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
  waiting, the offline event queue, local activity counters, and the last known extension budgets.
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
- From phase 4 an account has an email address, where the server sends email steps (section 8.3)
  and email reminders (section 11).

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

### 7.5 Secrets in the daemon's memory

The owner asked for this on 2026-11-18. The measures are Claude's call.

The daemon holds secrets in memory: the device token (section 7.2) from M3, and from phase 3 the
key of the local store (section 6.7). Other programs running as the same user are the main risk,
so the daemon:

- sets itself non-dumpable (`PR_SET_DUMPABLE` at 0), so those programs can't read its memory
  through `/proc` or attach a debugger, and a crash leaves no core dump with the secret in it;
- locks the memory pages that hold secrets with `mlock`, so they never reach swap;
- overwrites a secret with zeros as soon as it is no longer needed.

The owner suggested the kernel keyring as an option for secrets that only need to live for a
while. A key kept in the daemon's own process keyring stays in kernel memory, never reaches swap,
can expire on a timeout, and other processes can't read it. It doesn't help when two processes
need to share a secret, because that takes the user keyring, which every program of the same user
can read. Where the device token is stored between restarts is decided in M3.

The owner chose the libraries on 2026-11-20. `secrecy` wraps every secret, so it prints as
`[REDACTED]` in `Debug` output and logs and is read only through an explicit `expose_secret()`
call. `zeroize` overwrites the value when the wrapper is dropped. Zeroing clears only the current
copy, so code that handles a secret allocates it once at its final size and never clones or
formats it.

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
  message change from day to day, so the ADHD side doesn't learn to ignore them. The user's own
  messages vary too when the user writes them as a list of variants (section 8.3).

### 8.3 Actions and steps

Updated with the owner on 2026-11-20. The actions a rule takes are its steps:

- Show a desktop notification or warning (a notice).
- Send a message to the agent in open Claude Code sessions, with a level that limits what it takes
  on from then on (section 8.12).
- Lock the screen for X time, until a given time, or for the rest of a time window.
- Shut the machine down.
- Block an app (section 8.6).
- Inhibit a lock (section 8.4).
- From phase 4, send an email.

A rule can combine any of these, and none of them is required (owner). A rule with only a notice,
only an email, or a notice and an email, with no lock at all, is a normal rule. Two of the owner's
examples: after a long time in one app (section 8.1), a notice that says "you've used this app too
much, I recommend switching"; and an email at a set time, which reaches the phone while the
computer is off.

Night rules start from a default escalation, from notices to a lock to a shutdown, and every step
in it can be changed or left out.

When steps happen (Claude's call):

- Each step is placed relative to one of the rule's moments: the moment the rule fires (a time, an
  anchor or a condition, sections 8.1 and 8.2) and, when the rule has them, its lock and its
  shutdown. Examples: a notice 15 minutes before the lock, a Claude message 30 minutes before the
  shutdown.
- A step follows the moment it is tied to. The random window (section 8.2) picks the lock's minute,
  an extension moves the moment the lock comes back, and the shutdown counts from when the lock
  started, so steps tied to them move with them.
- Steps placed before the lock also come before the lock returns at the end of an extension, when
  they fit inside the extension. M1 already does this for notices.
- When the moment comes from a condition, such as two hours in one app, Selflow predicts it from
  the current activity, so the notices before it still arrive on time.

Shutdown (owner):

- A rule can shut the machine down with or without a lock before it. The owner wants it for urgent
  cases that aren't defined yet.
- Nothing is cut off without notice. The user picks the notices, but one always comes at least a
  minute before the shutdown, and that minute can't be turned off. M1 already keeps a minute
  between the lock screen, which shows the shutdown time, and the shutdown.
- A shutdown without a lock can't be postponed with an extension. With no lock screen there is no
  place for the unlock challenges, and an urgent shutdown isn't meant to wait. (Claude's call)
- The rule says whether a shutdown without a lock happens once or keeps happening (owner,
  2026-11-20). Once: it comes at its time, and turning the machine back on afterwards doesn't shut
  it down again. Repeating: until the rule ends (the end the rule sets, or else the next day
  boundary), every boot gives the minute of notice and shuts down again. Once is the default.
  (Claude's call)
- To keep the machine off with a way out, a rule can use a lock with a shutdown instead. That
  already repeats on every boot until the lock ends, and the unlock challenges are the exit.
- An exception day skips the rule either way, and on the same day it goes through the unlock
  challenges (section 8.9). Otherwise only a change to the configuration stops it, and loosening
  waits (section 8.9).

Notices that are hard to miss (owner, 2026-11-23). During the M2.1 checks the owner noticed a
notice three minutes late, because a desktop notification is easy to overlook. A notice can show
in two more ways, and the user configures both:

- as a critical notification that stays on screen until it is dismissed;
- as a `selflow-ui` popup window: centered, larger than a notification and calm.

Messages (owner):

- Notices, emails and Claude messages can all be the user's own text, per rule. When the user
  writes none, Selflow uses its default texts in the user's language.
- A custom message can be one text or a list of variants, and Selflow picks one per day, as it
  does with its own texts (section 8.2). (Claude's call)

Email, from phase 4 (Claude's proposal, confirmed by the owner):

- An email step goes to the address of the user's account (section 7.1).
- When the rule depends only on time (a time or an anchor), the server fires the email step itself,
  so it arrives with every device off.
- When the rule depends on something only a device can see, such as an app open for too long, the
  device reports the step to the server, and the server sends the email.

The server evaluates those time-only rules with the same `selflow-core` logic the daemon uses.
(Claude's call)

M1's configuration only knows the lock: `[[rule_set.lock]]`, with its notices in `warnings` and its
shutdown in `shutdown_after`. It is the first case of this model. The M2 plan replaces the format
with one that holds every kind of step. Selflow keeps each saved version as text, and a stored
version that no longer parses makes every enforcement tick fail, so that plan either converts the
versions M1 stored or starts from an empty store.

### 8.4 Inhibit

An inhibit only delays a lock and never cancels it. Its limits:

- It applies only to conditions the user configured on purpose, such as a video call app.
- Each inhibit has a maximum delay.
- Each inhibit counts against the extension budget of the lock it delays (section 8.5).
- Every inhibit is logged and shows up in the next-day report.

### 8.5 Lock screen and unlock challenges

The lock is a real Wayland session lock (`ext-session-lock`), so Alt+Tab or a workspace switch
can't get around it. The screen is calm and shows why it is locked, how long remains, and which
challenges are available.

Each rule has its own challenges, and its rule set gives the defaults the rule inherits (owner,
2026-11-20). The challenge types:

1. A mandatory wait, for example two minutes before the unlock option appears.
2. A written reason, which is logged and shown in the next-day report.
3. A long sentence the user has to type.
4. An extension budget: at most N extensions per day for each rule, each with a fixed length.
   When a rule's budget is gone, nothing unlocks that rule's lock. Each rule's budget is separate,
   so an app unblocked in the afternoon leaves the bedtime lock's extensions alone.
5. Optionally, a PIN that someone else holds.

The default is a wait, a written reason and an extension budget. A PIN or security question the
user knows by heart is never a default, because it doesn't interrupt an impulse.

The same challenges also work from a terminal, when the screen fails or when the user simply
prefers it (section 8.13).

### 8.5.1 Lock screen themes

The owner asked on 2026-11-20 for a lock screen as configurable as hyprlock's. The design below is
Claude's proposal, approved by the owner.

- The look lives in its own file, `~/.config/selflow/lock.toml`, apart from the rules. The session
  lock blocks input whatever the screen looks like, so a theme never weakens enforcement. Theme
  changes skip the loosening delay and the configuration versions (section 8.9), and the next lock
  uses the saved file.
- A theme is a list of widgets drawn with iced: a background (color, image, blur), labels (text,
  font, size, color), images, shapes, a countdown and the challenge form. Each widget has a
  position, an alignment and, optionally, the monitor it appears on.
- Text can use variables filled in by the daemon: `$label`, `$ends_at`, `$shutdown_at`,
  `$extension_in`, `$extensions_left` and `$time`. For example: `"$label until $ends_at"`.
- Some things a theme can't remove. The challenge form and the shutdown time always show: when the
  file leaves them out, Selflow draws them in their default place, so a theme can't hide the way
  out or a coming shutdown (section 8.3).
- Showing them also means they can be read. A theme can move and style those two, but Selflow
  keeps them inside the screen, at a minimum size, on a backing plate with enough contrast, so a
  theme can't make them tiny, push them off screen or paint them in the background color.
  (Claude's call after a review by another session, approved by the owner)
- A theme can't run commands, unlike hyprlock's `cmd[]`. A lock screen that runs scripts on a
  timer breaks the resource budget (section 15) and is a risk on a locked screen.
- Animations stay calm, with no flashing (section 4).
- Today's screen becomes the default theme, and a command such as `selflow lock theme init` writes
  a copy to start from. The full interface (section 13) edits themes later.
- It needs `selflow-ui` (section 6.1), so it comes at the end of M2 or in a small milestone right
  after it.

### 8.6 Blocking apps

The owner asked for a rule that keeps closing an app every time it opens, until they unblock it
through Selflow. The details below are Claude's call.

- When a window matching a block rule opens, the daemon closes it. The rule can instead terminate
  the app's process, because some apps keep running in the tray after their window closes.
- Hyprland reports new windows as events, so a block rule costs nothing while nothing opens.
- After closing the app, Selflow shows a notification saying the app is blocked. Clicking it, or
  opening the window (`selflow-ui`), starts the unblock flow.
- Unblocking goes through the block rule's challenges, lasts a fixed configured time, and counts
  against that rule's extension budget (section 8.5).

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
  warning, lock, app block or Claude level that has already started. Loosening means anything
  that weakens enforcement: a later time, a shorter lock, a disabled or deleted rule, a lock or
  shutdown step removed from a rule, a repeating shutdown changed to one that happens once
  (section 8.3), more or longer extensions, a shorter wait, fewer challenges,
  more inhibit, a higher workday limit, an anchor moved past its allowed shift (section 9), a
  Claude step removed, moved later, set to a lighter level or covering fewer folders
  (section 8.12), or a lighter mode (section 8.10). When the app can't tell whether a change
  loosens or tightens, it treats it as loosening.
- Changing a notice, an email or the text of a message is neutral and takes effect right away.
  (Claude's call)
- So is adding, deleting or moving a rule whose only steps are notices or emails, because it
  enforces nothing. The "deleted rule" above means a rule with a lock, a shutdown, a block or a
  Claude step. (Claude's call, approved by the owner on 2026-11-20)
- Each change in a save keeps its own clock (owner, 2026-11-20). The stricter and neutral changes
  in a save take effect right away, and its loosening changes wait their 24 hours. A later save
  doesn't restart the wait of a change already waiting: it only adds its own changes, each with
  its own time. Changes that depend on each other, such as a lighter mode and a value only that
  mode allows, take effect together at the later of their times. (That last point is Claude's
  call.)
- The 24-hour delay is configurable, and shortening it counts as loosening.
- The server keeps every saved configuration and the time each of its changes takes effect.
  `selflow config status` shows the changes still waiting, and the next-day report lists them.
- Exception days (a holiday, a sick day, an on-call night) pass without friction when scheduled at
  least 24 hours ahead. On the same day, they go through the unlock challenges and show up in the
  report.
- If a wrong rule locks the screen at a bad moment, such as during a meeting, the unlock
  challenges are the emergency exit.

### 8.10 Strictness modes and onboarding

The owner asked for a mode, lighter or stricter, chosen during onboarding. Reworked with the owner
on 2026-11-20: a mode never stops the user from adding a rule, and it doesn't define any rule or
behavior either. Every kind of step and every Claude level (section 8.12) is available in every
mode. A mode does two things:

1. It sets the defaults: the rules a new configuration starts with, and the value a field takes
   when the user leaves it out.
2. It sets how far the user can loosen. In Strict, for example, the unlock wait can't go below a
   floor, while in Light it can go much lower.

How the limits work (Claude's call, confirmed by the owner):

- They apply only when loosening. A stricter value never runs into them.
- Selflow checks a configuration against the limits of the mode it names. Saving one with a value
  beyond a limit fails with a clear message in the user's language that names the mode and the
  limit, like the other validation errors (section 8.11).
- Limits can cover the unlock wait, the number and length of extensions, the loosening delay
  (section 8.9), and how close to a lock or a shutdown the first notice can come.
- Switching to a lighter mode counts as loosening (section 8.9), so the lighter limits only apply
  once the switch takes effect.

The three presets are Claude's call:

| Mode | Defaults | Limits |
|---|---|---|
| Light | Notices and locks with a short wait to unlock, a generous extension budget, no shutdown step, a short loosening delay, Claude steps that only send messages | Loose: the wait and the loosening delay can go low, and the extensions can go high |
| Balanced | The defaults described in this document: a wait, a written reason and a small extension budget, a shutdown after a long ignored lock, a 24-hour loosening delay, Claude steps at "Light tasks only" and then "No new work" | Moderate floors on the wait and the loosening delay, and a cap on extensions |
| Strict | Longer waits, no extensions for the bedtime lock, an earlier shutdown, a loosening delay longer than 24 hours, and a last Claude step at "Save only" | High floors on the wait and the loosening delay, and a low cap on extensions |

The builder picks the exact numbers, and they get tuned during real use.

In phase 1, onboarding runs in the terminal as `selflow init`. It asks for the server address and
the login, then registers the device. For a user with no configuration yet, it also asks for the
language, the mode, work hours, bedtime and the main apps to watch. A graphical onboarding comes
with the full interface in phase 6.

### 8.11 Editing rules and reading reports in phase 1

Approved by the owner.

- Each user's configuration is a human-editable text file, in the spirit of `hyprland.conf`.
- `selflow config edit` downloads the current version from the server, opens it in `$EDITOR`,
  validates it and uploads it. The server then applies section 8.9. Validation errors are clear
  and come in both languages.
- The next-day report arrives as a desktop notification in the morning, and `selflow report`
  shows the full report in the terminal.
- The full interface (section 13) comes near the end of the plan, in phase 6.

### 8.12 Claude Code sessions

Approved by the owner on 2026-11-18 and reworked with the owner on 2026-11-20. The owner's longest
work stretches often happen inside Claude Code sessions, and a desktop notification is easy to
ignore there. So before a rule's lock or shutdown, Selflow sends a message to the agent in each
open session the rule covers, and each message can narrow what the agent takes on. Hearing it from
the tool they are working in is harder to brush off than a notification, and the agent can write
down where the work stopped, so letting go doesn't mean holding the whole task in their head until
morning.

The owner's example: they are about to leave work, don't notice the time, and ask Claude for
something that takes an hour. A first message limits Claude to light tasks, and a later one asks it
to refuse new work, so the session winds down with the workday instead of running past it.

Agent steps go through adapters, one per agent tool (owner, 2026-11-27). The owner works in Claude
Code most of the time and switches to another agent, such as opencode, when the usage limit runs
out. Claude Code is the first adapter, and the M2b plan shapes the integration so that another
agent can be added without touching the rules. A rule only says "send this to the agent".

What Claude Code offers, checked against the official documentation on 2026-11-18 (Claude Code
2.1.327 on the owner's laptop):

- Each session binds an inbox socket for cross-session messages, and its path is exported to hooks
  as `CLAUDE_CODE_MESSAGING_SOCKET`. A message sent to an idle session starts a new turn. This
  needs Claude Code 2.1.224 or later. On Linux the socket is restricted to the user's own account
  (on the owner's laptop, `/run/user/1000/cc-socks/` has mode 0700 and each socket 0600). Claude
  Code closes a connection that hasn't sent a complete line within 30 seconds, so the daemon opens
  it only when the message is ready. The documentation doesn't describe the framing of a message,
  so the M2b plan tested it against a live session on 2026-11-28 (Claude Code 2.1.332). A message
  is one line of JSON ending in a newline:
  `{"type":"user","message":{"role":"user","content":"<text>"}}`. On Linux it went through with
  no auth line; Claude Code's own log hint marks the auth line as required on some sockets, so
  that part holds for Linux only. It woke an idle session.
  Claude Code showed it as a message from another session and ran the `UserPromptSubmit` hook
  with the text as the prompt. The socket lives at `$XDG_RUNTIME_DIR/cc-socks/<pid>.sock`.
- Claude Code also exports a per-session token, `CLAUDE_CODE_MESSAGING_TOKEN`. Selflow doesn't use
  it. The token has two jobs: access control on native Windows, where the socket can't be limited
  to the user's account, and proving that a message came from one of the session's own child
  processes when Claude Code has no process evidence for that. On Linux, sending it is optional,
  and `selflowd` is started by systemd, so it isn't a child of the session anyway. Leaving the
  token out means Selflow holds no secret for this feature.
- Inbound controls decide what happens to the message. When the session has no
  `crossSessionInbound` value, a session that prompts for permissions (auto mode counts as
  prompting) receives it, and a session that bypasses permission prompts holds it behind an
  approval dialog. A `hold` or `refuse` value holds or drops it. A session that bypasses
  permission prompts needs `crossSessionInbound` at `accept` to receive Selflow's messages.
- Hooks run only on session events, so they can't wake an idle session. `SessionStart` and
  `SessionEnd` fire when a session opens and closes. `UserPromptSubmit` can add context to the
  user's prompt. `PreToolUse` can deny a tool call, and Claude reads the reason given.
- Channels (`--channels`) can also push messages, but every session has to be started with a flag,
  and a custom channel needs a development flag during the research preview. Selflow doesn't use
  them.

A message to Claude is one more kind of step in a rule (section 8.3), next to notices, the lock
and the shutdown, so any rule can use it, such as bedtime or the end of the workday (owner). It
lives in the user's configuration file, so the loosening rule (section 8.9) covers it.
Each Claude step has:

- a time, placed like any other step, for example 60 minutes before the rule's lock or 30 minutes
  before its shutdown (section 8.3); in M2b only before the rule's start, see the paragraph
  below;
- a message, with default texts in the user's language that the user can replace (owner);
- a level, which sets what Claude takes on from that step on (owner);
- optionally, a list of folders. A folder covers every session whose working directory is inside
  it, and `~` covers the whole home folder. A step with no folders covers every session.
  (Claude's call)
- optionally, a switch that makes it cover runs nobody is watching too. It is off by default
  (owner, 2026-11-20).

For M2b, a Claude step's time counts back only from the rule's start, not from its lock or its
shutdown. Unlike a notice, which section 8.3 repeats before the lock returns at the end of an
extension, a Claude step's message isn't sent again then; the level just holds through the
extension. This narrows section 8.3, which allows steps placed before the lock or the shutdown and
repeats them before the lock returns. Holding the
level through the extension keeps enforcement at least as strict, and tying steps to the start
keeps M2b smaller. If the owner wants the section 8.3 behavior, a later change adds times counted
from the lock or the shutdown and the repeat. (Claude's call, pending the owner's confirmation)

Claude steps cover the sessions someone is typing in (owner, 2026-11-20). Runs nobody is watching,
such as `claude -p` in a script, an Agent SDK program or a scheduled run, carry on as normal unless
the step turns that switch on. The next-day report lists the ones that ran while a level was
active, so using them to get around "No new work" shows up the next day. An interactive session
left running on its own still counts as attended, and a step's folder list can leave it out.
A hook tells the two apart by `CLAUDE_CODE_SESSION_ATTENDED`: `1` in an interactive session and
`0` in a `claude -p` run (checked on 2026-11-28, section 21).

The levels go from lightest to firmest. The owner described the middle two, and the rest is
Claude's call:

| Level | What Claude is asked to do | How Selflow holds it |
|---|---|---|
| Message only | Pass the message on. Nothing else changes. | The message. |
| Light tasks only | Finish what is underway and take on only small tasks. | The message, and from then on every prompt carries a reminder of the time and the level. |
| No new work | Take on nothing new. Help save the work, write down where it stopped and what the next step is, and tell the user to go rest. | The message and the reminder on every prompt. |
| Save only | Everything in No new work. | Also a `PreToolUse` hook that denies file edits and shell commands. A configurable list of commands stays allowed so the work can be saved, by default `git status`, `git diff`, `git add`, `git commit` and `git stash push`. |

How levels behave (Claude's call):

- A level holds from its step until the rule ends: the end of its lock, the end the rule sets, or
  otherwise the next day boundary (the `day_starts_at` the lock uses). It also holds through an
  extension, while the lock is away.
- When several levels cover one session, the firmest wins. For example, an end-of-work rule can
  put the work repositories at "Light tasks only" at 17:30, and the bedtime rule can put every
  session at "No new work" later that night.
- Sessions that no step covers get nothing.

Default steps for the bedtime rule (Claude's call), covering every session:

1. Wind down, 60 minutes before the lock, at "Light tasks only".
2. Wrap up, 15 minutes before the lock, at "No new work".

Every level is available in every mode. The mode only sets the default levels and how far a step
can be loosened (section 8.10): Light sends both default steps as messages only, and Strict sets
the second one to "Save only". The lock (section 8.5) still comes at its time, whatever the
sessions are doing.

How it works (Claude's call):

1. A `SessionStart` hook runs `selflow claude hook session-start`. The command reads the session
   id and working directory from the hook input and the socket path from the environment, and
   passes them to `selflowd` over the daemon's local socket.
2. A `SessionEnd` hook removes the session. The daemon also drops sessions whose socket is gone.
   The prompt hook sends the same data as the start hook, so sessions opened before a daemon
   restart register again on their next prompt.
3. When a Claude step fires, `selflowd` sends its message to every registered session the step
   covers.
4. A `UserPromptSubmit` hook runs `selflow claude hook prompt`. It asks the daemon which level
   holds for the session's folder, and when it is "Light tasks only" or firmer, it prints the
   reminder as additional context.
5. A `PreToolUse` hook runs `selflow claude hook tool`. When the level is "Save only", it denies
   file edits and shell commands outside the allowed list, with a reason Claude can pass on.
6. Hook commands answer within a short timeout. When the daemon can't be reached, they let the
   prompt or the tool call through. Stopping the daemon already shows up as a gap (section 8.7).

Selflow installs the hooks itself, because the owner wants to set this up through Selflow rather
than by hand. `selflow claude install` adds Selflow's hook entries to `~/.claude/settings.json`,
which applies to every session, merges them with the user's own hooks, and shows the change
before writing it. It also checks the Claude Code version, and it warns when the user's settings
would keep the messages from arriving: `crossSessionInbound` at `hold` or `refuse`, or permission
prompts bypassed by default without `accept`. Running it again updates the entries, and
`selflow claude uninstall` removes only Selflow's. Removing the hooks by hand is a bypass Selflow
can't prevent, in the same spirit as section 8.7. The full interface (section 13) edits the
Claude steps later on.

Selflow never reads transcripts or prompts. Events record the rule, the step, the level, the
folder, and whether the message was delivered or a tool call was denied. The next-day report lists
them.

### 8.13 Unlocking from the terminal

The owner asked for this on 2026-11-20. The lock screen can fail: its client dies, freezes or stops
taking input, and the compositor keeps the session locked. So the user can also lift a lock from
another TTY (for example Ctrl+Alt+F4), through the same challenges as the screen. The owner wants
it as an everyday option too: on any night, the user can pick the screen or the terminal.

How it works (Claude's proposal, confirmed by the owner):

- `selflow unlock` talks to the daemon over its local socket and asks for the same things as the
  screen: the wait, the written reason, and the phrase when the rule set has one.
- It draws on the same budget as the screen, the one of the rule whose lock is on, and grants the
  same length.
  The terminal is never an easier way out than the screen.
- Once the daemon grants it, the screen really unlocks. The compositor may still be holding the
  lock for a dead or frozen client, so the daemon stops that client and starts a new one that takes
  over the lock (Hyprland's `allow_session_lock_restore`, through the adapter in section 6.4) and
  releases it at once. Without that step, the daemon would count an extension while the screen
  stayed locked.
- The report says the extension came from the terminal, with its reason, so a screen that keeps
  failing shows up the next day.
- When the daemon can't be reached, the command can't check the challenges, so it doesn't unlock.
  It says how to restart the daemon instead. Stopping the daemon is already a known gap
  (section 8.7).

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
- In phase 2 the window (`selflow-ui`) gains a routine view with today's timeline and the re-plan
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

- Email, sent by the server over SMTP to the account's address (section 7.1): email steps in rules
  (section 8.3), and reminders on a fixed schedule with an optional random window. Their main job
  is reaching the phone at night while there is no phone client.
- URL and site rules, through a browser extension that reports the active tab to the local daemon.
- Plugins, through a standard detector contract: a plugin watches something and reports it to the
  daemon. The builder picks the plugin mechanism (WASM, external processes or something else)
  within the resource budget. The owner plans to widen plugins beyond detectors when this phase
  starts.
- Optional two-factor login with TOTP (section 7.1).
- Trusted contacts over WhatsApp: people the user trusts set off rules with ordinary messages,
  inside time windows, and can grant time on the locks they caused. Designed in
  `2026-11-27-selflow-trusted-contacts-design.md`.

## 12. Tasks and reminders (phase 5)

Configurable tasks and reminders, with times written in natural language in the style of Todoist
("tomorrow at 3pm", "every Monday"), in English and pt-BR. Tasks connect to the "smallest first
step" behavior in section 9. This phase is not the focus and comes late on purpose.

## 13. Full interface (phase 6)

The owner wants the graphical interface near the end of the plan. It covers rules, rule sets, the
routine, medications, challenges and modes, along with a graphical onboarding and a report view.
The text file stays supported, and changes made in the interface follow the same loosening rules
(section 8.9).

The owner wants it to be the complete interface rather than an editor for the text file (owner,
2026-11-27). It hides some of the machinery behind simpler choices, so that nobody has to read
hours of documentation to set Selflow up, which is a lot to ask of someone with ADHD. This phase
gets its own design before it starts.

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
5. The daemon has no GUI. `selflowd` doesn't link iced at all. iced runs only inside `selflow-ui`
   (inside `selflow` until M2), for the window and the lock screen, and only while one of them is
   on screen. From M2 the `selflow` command line links no GUI library either, so the Claude Code
   hooks it runs on every prompt and tool call start a small binary (section 8.12).

The targets have to be measured, not assumed. The daemon should use close to 0% CPU when idle and
stay in the low tens of MB of RAM. The server process should run comfortably on the Pi next to the
owner's other services. Each phase counts as done only after measuring the daemon on the owner's
laptop and the server on the Pi. Each `selflow claude hook` call should finish within 10 ms at the
95th percentile on the owner's laptop, counting the exchange with the daemon (owner, 2026-11-26).
M2b counts as done only after measuring it.

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
   - the daemon, the tray, the small window and the lock screen, with the graphical parts in
     `selflow-ui` and a `selflow` command line that links no GUI library (section 6.1);
   - the Hyprland adapter, i18n and the systemd units;
   - terminal onboarding with modes as defaults and limits (section 8.10), and the text
     configuration with `selflow config edit` and `selflow config status`;
   - configuration versions, the loosening rules and exception days;
   - rules by window, process, time, anchor and workday, with any mix of steps, including rules
     with no lock and a shutdown with no lock, custom messages (section 8.3), and reusable rule
     sets;
   - app blocking, unlock challenges, the terminal unlock (section 8.13), escalation up to
     shutdown, and bounded inhibit;
   - enforcement per user across devices, and the cross-device workday with its schedule fallback;
   - the next-day report;
   - Claude Code steps with levels, in any rule (section 8.12);
   - deployment on the Pi behind HTTPS.

   Inside this phase, the plan should first reach a working night lock on one machine, then add
   the server and the multi-device pieces, so the owner has a usable tool early. (Claude's call)
2. **AuDHD routine.** Section 9, including the routine view.
3. **Medication.** Section 10, plus encryption of the local store (section 6.7).
4. **Reach.** Section 11.
5. **Tasks and reminders.** Section 12.
6. **Full interface.** Section 13.
7. **Future.** A phone client (iPhone first) and other desktop environments. Two ideas from the
   owner wait here too, and each needs its own design first (section 21):
   - AI help: Selflow hands small decisions to an AI, through a Claude Code hook, an API token or a
     local model. The owner's example is an AI that judges the reason given for an extension.
   - Profiles: the user picks a profile, and a set of rules is active while it is.

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
| Secrets in memory (from M3) | `secrecy` 0.10.3 and `zeroize` 1.9.2 | Checked 2026-11-20. `zeroize` released 2026-11-17 from the active RustCrypto repository. `secrecy` has had no release since October 2024, but its repository is active (last commit 2026-09-02) and OSV lists no advisories |

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
│   ├── selflow-i18n/       # fluent loading, used by every binary
│   ├── selflow-platform/   # from M2: operating system code the binaries share (paths, local connections, private files)
│   ├── selflow-server/     # binary `selflow-server`: server and admin CLI; sql/schema/ holds the PostgreSQL migrations
│   ├── selflowd/           # binary `selflowd`: the daemon
│   ├── selflow/            # binary `selflow`: the command line (it also holds the lock screen until M2)
│   └── selflow-ui/         # from M2, binary `selflow-ui`: window, `--tray`, lock screen
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

Dependency direction: `selflow-core`, `selflow-i18n` and `selflow-platform` depend on no other
workspace crate.
`selflow-protocol` may depend on `selflow-core`. The binary crates depend on the libraries and
never on each other. From M2, the code that `selflow` and `selflow-ui` share for talking to the
daemon lives in a library crate.

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
├── os/             # from M2: operating system calls, shutting down and the signals that stop the daemon
├── store/          # local SQLite store: config cache, offline queue, counters
├── sync/           # server client, push channel, heartbeat
└── ipc/            # local socket for the tray and the window
```

Operating system code (from M2; Claude's call, approved by the owner on 2026-11-20):

The owner asked for a layout that keeps a later port to another operating system, such as Windows,
clean. Code that depends on the operating system stays in a few known places. Inside each one, a
file per system holds the implementation, and `#[cfg]` picks it at compile time. Linux is the only
system for now, so each place has a `unix.rs`. A port adds a `windows.rs` beside it and leaves the
code that calls it alone.

```
crates/selflow-platform/src/
├── lib.rs
├── paths/          # where the store, the config folder (lock.toml), the runtime files and the daemon's socket live; unix.rs reads XDG_STATE_HOME, XDG_CONFIG_HOME and XDG_RUNTIME_DIR
├── transport/      # listening for and opening local connections to the daemon; unix.rs uses a Unix socket
└── secure_fs/      # folders and files only the user can open; unix.rs sets modes 0700 and 0600
```

- `selflow-platform` holds what more than one binary needs. Code that only one binary needs stays
  in that binary, under `src/os/`. In the daemon, `os/` shuts the machine down and listens for the
  signals that stop the daemon, while the enforcer still decides when to shut down. In the command
  line, `os/` starts the editor for `selflow config edit`.
- `selflow-core`, `selflow-i18n` and `selflow-protocol` hold no operating system code.
  `selflow-protocol` reads and writes messages on a stream it is handed and never opens one, and
  whoever opens the stream does it through `selflow-platform`. `make check` also builds these three
  crates for a Windows target (for example `x86_64-pc-windows-msvc`), so Linux-only code that slips
  into them fails the check.
- The lock screen keeps its model, view and style apart from the session lock itself. The Wayland
  session lock (`ext-session-lock` through `iced_exwlshell`) lives in
  `selflow-ui/src/lock/backend/wayland.rs`, and another system adds its own file in `backend/`.
- A dependency that builds on only one system goes in a `[target.'cfg(...)'.dependencies]` table of
  the crate that uses it, such as `iced_exwlshell` under Linux.

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

- The module layout inside `selflow-core`, `selflow-platform`, `selflow-server`, `selflow` and
  `selflow-ui`, following section 19.
- The daemon-to-server protocol and the push mechanism, within section 6.6.
- The database schemas, and the PostgreSQL driver, SQLite driver and migration tool, each checked
  on crates.io before it is added. The owner's diario-alerr uses `tokio-postgres` with `deadpool`.
- The details of the Hyprland IPC.
- The format of the configuration file, as long as it is human editable, allows comments, gives
  clear errors in both languages and holds every kind of step (section 8.3). That includes the
  names of the Claude levels in the file (section 8.12).
- The exact defaults and limits of the strictness modes (section 8.10).
- Which library crate holds the code `selflow` and `selflow-ui` share for talking to the daemon: a
  new one or `selflow-protocol` (section 19). In `selflow-protocol`, that code works on a stream
  the binary opens through `selflow-platform`.
- Whether the tray uses `ksni` directly or through `tray-icon` with its `ksni` feature. Checked on
  2026-11-20: `tray-icon` 0.26.1 with its default features off and `ksni` on runs on `ksni` under
  Linux and pulls no GTK, and it also supports Windows, so the tray would need no port. Under Linux
  its menus go through a snapshot mode nobody has tried on Hyprland yet, so the M2 plan tries it
  first and keeps `ksni` directly if the menus misbehave.
- Whether an email step that an offline device reports late is still sent (section 8.3).
- How a saved configuration holds changes that take effect at different times (section 8.9). It
  shapes the server's version schema in M3.
- The plugin mechanism (section 11).

## 21. Open questions

- The visual design of the routine view and of the window (`selflow-ui`). To settle before phase 2,
  ideally with mockups.
- The Pi's architecture and operating system, and how it is reached from the internet. To check
  before the deploy work in phase 1.
- Answered on 2026-11-28: the message framing on the Claude Code inbox socket (section 8.12). An
  experiment against a live session (Claude Code 2.1.332) found it: one JSON line,
  `{"type":"user","message":{"role":"user","content":"<text>"}}` and a newline. On Linux it needed
  no auth line or token; Claude Code's log hint marks the auth line as required on some sockets,
  so that part was verified on Linux only. Sent from a process outside the session, it woke the
  idle session. Messages therefore
  travel through the inbox, and the hooks remain for reminders, denials and sessions the daemon
  didn't know about.
- Answered on 2026-11-28: how a hook tells a session someone is typing in from a run nobody is
  watching (section 8.12). Inside hooks, `CLAUDE_CODE_SESSION_ATTENDED` is `1` and
  `CLAUDE_CODE_ENTRYPOINT` is `cli` in an interactive session; in a `claude -p` run they are `0`
  and `sdk-cli`. Selflow counts a session as unattended only when `CLAUDE_CODE_SESSION_ATTENDED`
  is `0`; a missing variable counts as attended, the strict side.
- Profiles (phase 7). Before they are designed, settle how switching to a profile relates to
  skipping a rule on purpose, so that a profile switch never becomes the easy bypass section 4
  rules out.
- AI help (phase 7). What an AI may see, since an extension reason is private text, and whether it
  runs as a local model or through a hook or a token, which decides whether that text leaves the
  machine.

Designs still to come, recorded with the owner on 2026-11-27. Each one gets its own design before
it is built:

- Minigames as an unlock challenge. The owner has seen other focus apps offer minigames to get out
  of a block. Here they would be a sixth challenge type next to the five in section 8.5, with games
  that are Selflow's own. The central question: section 4 rules out an easy bypass, and a game that
  is too much fun turns unlocking into a reward instead of friction.
- Feature flags. A flag turns a whole area off, and the area then disappears from the
  configuration, the onboarding, the interface and the reports. The owner's example is AI: some
  people get really annoyed by AI features, so they can turn them all off. Other likely areas are
  trusted contacts, medication and minigames. Flags are separate from modes (section 8.10),
  which only set defaults and limits. To settle in that design: switching an area off can weaken
  rules that are active (turning trusted contacts off while only a family member can end a lock),
  so it probably counts as loosening (section 8.9).
