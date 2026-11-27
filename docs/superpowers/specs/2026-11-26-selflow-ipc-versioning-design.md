# Selflow: versioned IPC and restart-safe daemon

- **Status:** approved by the owner on 2026-11-26
- **Created:** 2026-11-26, with the answers to the builder check (restart vs. reboot in section
  6.5, the restart decision in section 6.3) and the owner's security pass (write before acting in
  section 6.5, the binary taken from the service manager in section 6.3, the daemon-side handshake
  timeout in section 6.2)
- **Relates to:** the main design (`2026-11-17-selflow-design.md`), sections 6.1 (components),
  7 (security), 8.7 (tamper resistance) and 15 (resource budget). It changes the local socket
  between `selflowd` and its clients only. The server protocol (section 6.6) is out of scope.

Decisions marked "Claude's call" were settled by Claude while writing this document and are open
to the owner's veto. Everything else was decided by the owner in the design conversation of
2026-11-26.

## 1. Why

The owner wants the local protocol to be binary. A binary format with no field names breaks as
soon as the daemon and a client disagree on the message types, and that happens after every
update: the running `selflowd` keeps the old code until something restarts it, while the new
`selflow` and `selflow-ui` are already on disk. Today the daemon restarts only at login, from the
`hyprland.start` hook in `contrib/hyprland.lua`.

So the clients have to notice the mismatch and restart the daemon themselves. That only works if a
restart changes nothing about how the rules behave, and today it does. The daemon rebuilds most of
its state from SQLite on every tick, but four things live only in the engine's memory
(`crates/selflowd/src/engine/mod.rs`, struct `Engine`):

| Field | What it remembers | What a restart does today |
|---|---|---|
| `announced` | when a shutdown was announced | The shutdown is announced again and its time is recomputed as restart + `SHUTDOWN_NOTICE` (60 s). Every restart postpones the shutdown, so restarting in a loop postpones it forever. |
| `shown` | each lock on screen, with the shutdown time it displays (`shutdown_on_screen`) | The countdown on the lock screen jumps later, for the same reason. A lock that ended while the daemon was down never gets its `LockEnded` event, because `release()` only walks `shown`. |
| `sent` | notices already sent | Notices sent in the last `WARNING_GRACE` (120 s) are sent again. |
| `shut_down` | rules that already shut the machine down | Probably nothing, because `RuleShutdown` events are stored and the core reads them through `DayLog::shutdowns`. Not verified end to end. |

These gaps exist today for any restart: a crash under `Restart=always`, a manual
`systemctl --user restart`, a login. Automatic restarts only make them more frequent. The owner
wants all of them closed in this work, including the shutdown postponement, which is a way around
a rule (section 8.7).

## 2. Goals

1. The local socket carries `postcard`-encoded messages in length-delimited frames.
2. Every connection starts with a fixed handshake that carries the protocol version, readable by
   any past or future version.
3. When a client finds a daemon on a different protocol version, it restarts the daemon and
   retries, but only when the installed daemon binary already speaks the client's version.
4. A daemon restart, at any moment, changes nothing a rule does: same shutdown time, same lock
   countdown, no repeated notices, every started lock eventually ended in the event log. A reboot
   still starts the per-boot behaviour over, as the main design wants.
5. The daemon refuses connections from other user ids.

## 3. Non-goals

- The server protocol. The owner may later move sync into a separate client binary, maybe over
  gRPC or plain HTTP. That decision is open (section 9) and must not be constrained here, so the
  local protocol types stay private to the local socket and are never reused for the server.
- Checking which program connected (path of the peer's executable). The command line is a public
  entry point, so any process can run `selflow` instead of talking to the socket, and pid-based
  checks race with pid reuse. The real protection is that the daemon applies the same challenges to
  every request.
- A separate socket for plugins. Noted for phase 4 in section 9.
- Changing how the lock screen behaves when its client dies. The compositor keeps the session
  locked (main design, section 8.13), and the owner accepted the two or three seconds of the
  compositor's fallback screen during a restart mid-lock.

## 4. Anti-goals

- A restart that lets a lock, a shutdown or a notice behave differently from an uninterrupted run.
  This is the failure the owner cares about most.
- A restart loop. A client that keeps restarting the daemon because the versions never match would
  leave enforcement off for as long as it runs.
- Saving state "on exit". A crash, a `SIGKILL` after systemd's stop timeout, a power cut or the
  daemon's own shutdown skip any exit code. Everything is written when it happens, the way the
  engine already writes its records at the end of each tick.

## 5. Constraints

- Project rules from `CLAUDE.md`: no comments in code, one module per subject with its own
  `thiserror` enum, operating system code in `selflow-platform` (shared) or a binary's `src/os/`,
  none in `selflow-core`, `selflow-i18n` or `selflow-protocol`, and `make check` builds those three
  for Windows. Every new dependency is checked on crates.io first.
- Resource budget (main design, section 15). Nothing here may add a periodic task to the idle
  daemon.
- Every end-to-end test runs inside the isolated environment
  (`.claude/skills/testing-selflow-in-isolation/`). Its `systemctl` is a stub, so the restart path
  needs a way to be exercised there (section 7).
- `main` already has commit `6749ee0`, which frames messages with `tokio-util`'s `LinesCodec`.
  This work replaces that framing.

## 6. Design

### 6.1 Wire format

- Messages are encoded with `postcard` (1.2.0, checked on 2026-11-26: latest release that week,
  repository active, not deprecated).
- Frames use `tokio_util::codec::LengthDelimitedCodec` with a maximum frame length of 1 MiB, the
  same limit `MAX_LINE_BYTES` sets today. `tokio-util` is already a dependency.
- `postcard` can't decode internally tagged enums: its `deserialize_any` always returns
  `UnsupportedDeserAny`. So `#[serde(tag = "type")]` comes off `Request` and `Response`, and
  `#[serde(tag = "refusal")]` comes off the refusal enum in
  `crates/selflow-protocol/src/ipc/types.rs`. They become serde's default externally tagged enums.
- `#[serde(default)]` on `RequestExtension::source` existed to tolerate older clients. The
  handshake makes that unnecessary, and `postcard` ignores it anyway, so it goes.
- `postcard` encodes enum variants by position and struct fields by order. Adding, removing or
  reordering a variant or a field changes the wire format, and any such change bumps the protocol
  version. A test encodes one sample of every request and response and compares the bytes with a
  stored copy, so a type change that forgets the bump fails `make check`. (Claude's call.)

### 6.2 Handshake

- Right after connecting, the client writes 8 raw bytes: the magic `SLFW` followed by
  `PROTOCOL_VERSION` as a little-endian `u32`. The daemon answers with the same 8 bytes carrying its
  own version. Both sides compare before any `postcard` frame is exchanged. (The byte layout is
  Claude's call. What the owner chose is a fixed handshake outside the message format, so that two
  versions can always read each other's version.)
- `PROTOCOL_VERSION` is a constant in `selflow-protocol`, starting at 1.
- If the versions differ, the daemon closes the connection after sending its 8 bytes.
- The daemons running today speak JSON lines and know no handshake. They read until a newline, so
  they never answer the 8 bytes. The client treats "no complete handshake within 2 seconds" or a
  closed connection as a mismatch. This is what makes the first update to the new protocol restart
  the old daemon. (The timeout value is Claude's call.)
- The daemon applies the same 2 seconds the other way: a connection that hasn't sent its 8 bytes
  by then is closed, so nothing can hold connections open without speaking. Connections that
  completed the handshake, such as the lock client's subscription, stay open as long as they need.
  (Claude's call, accepted by the owner.)

### 6.3 Automatic restart

A restart only helps when the daemon binary installed on disk already speaks the client's version.
So the client checks that first, and restarts only when it holds. (Claude's call, accepted by the
owner, to meet the restart-loop anti-goal.)

1. On a mismatch, the client asks the service manager which binary a restart would start. On
   Linux that is the `path=` field of `systemctl --user show selflowd.service -p ExecStart
   --value`, which systemd prints with the unit's specifiers already resolved (on the owner's
   laptop, `path=/home/talinux/.cargo/bin/selflowd`). This works wherever a package installs the
   daemon, `/usr/libexec` included, and it checks exactly the binary the restart will run. The
   client never searches the `PATH` and never guesses a sibling path. (Claude's call, accepted by
   the owner.)
2. The client runs that binary with `--protocol-version`. That flag prints `PROTOCOL_VERSION` and
   exits, without binding the socket or opening the store.
3. If the binary speaks the client's version, the client restarts the daemon, waits for the socket
   to accept connections again, and retries the handshake.
4. If it doesn't, a restart would bring back a daemon that still doesn't match, so the client
   doesn't restart. It reports both versions and says the installed binaries don't match. This is
   the case of a partial install, or the middle of `make install`, which installs `selflowd`,
   `selflow` and `selflow-ui` one after another. The same applies when there is no unit to ask:
   without one, a restart through the service manager couldn't work either.
5. On top of that, a client restarts the daemon at most once per invocation, in case the binary
   changes between the check and the restart.

Who does this, and what each one does when it can't fix the mismatch:

- `selflow` and `selflow-ui` (window and tray) show the error and stop.
- The lock client exits with an error. It is started by the daemon from the binary next to the
  daemon's own executable, so after an update an old daemon can start a new lock client, which
  then restarts the daemon. That is the restart mid-lock the owner accepted. When it can't fix the
  mismatch, `LockSupervisor` starts it again with its existing back-off (1 to 30 seconds), and the
  attempt after the install finishes succeeds. Meanwhile the compositor keeps the session locked,
  and `selflow unlock` is the way out (main design, section 8.13).

Where the code lives:

- Asking which binary the unit starts and restarting it are operating system code, so they live
  in `selflow-platform`. On Linux the restart is
  `systemctl --user --no-block restart selflowd.service`, for the unit in
  `contrib/selflowd.service`. `--no-block` matters for the lock client: it runs inside the
  service's cgroup, so stopping the service kills it and the `systemctl` it started. With
  `--no-block`, `systemctl` returns as soon as systemd has queued the restart, and systemd carries
  the restart through after the caller is gone.
- The check and the restart sit in `selflow-client`, which all three binaries use, so they are
  written once.

### 6.4 Peer user check

- On every accepted connection the daemon reads the peer's credentials (`tokio`'s
  `UnixStream::peer_cred`, no new dependency) and closes the connection, with a warning in the log,
  if the uid differs from its own.
- This is a second barrier behind the mode 0700 of `$XDG_RUNTIME_DIR` (on the owner's laptop,
  `/run/user/1000` is `700`). It lives in `selflow-platform`'s Unix transport. Windows is out of
  scope until a Windows transport exists.

### 6.5 State that survives a restart

A daemon restart and a reboot are different events, and the main design wants them to behave
differently. After a restart nothing may change. After a boot, some shutdowns must start over: a
repeating shutdown rule gives a fresh minute of notice and shuts down again, and so does a lock
with a shutdown (main design, section 8.3). Today a boot works by accident, because the engine's
memory starts empty. Restoring everything from SQLite would break it, so the persisted state has
two lifetimes. (Claude's call, accepted by the owner.)

- **Per boot:** shutdown and notice bookkeeping. It is stored with the current boot id and only
  rows with the current boot id are loaded. A restart in the same boot gets everything back. A new
  boot starts empty, as the main design wants. Reading the boot id is operating system code in
  `selflow-platform`. On Linux it is `/proc/sys/kernel/random/boot_id`.
- **Across boots:** which locks have started and not yet ended, so that their `LockEnded` event is
  always written.

A boot id is used instead of `$XDG_RUNTIME_DIR`, which also empties on logout. With the runtime
directory, logging out and back in would reset the bookkeeping and postpone the shutdown again.

Each item is written at the moment it happens and read back when the engine starts. Loading covers
the same two logical dates `night_log` reads (yesterday and today).

Every item is written before the action it describes. Today `announced_shutdown` sends the
shutdown notice first and records the announcement after it. If the daemon died between the two,
the announcement would be lost, the next daemon would announce again, and the shutdown would move
60 seconds later. That is the postponement this work closes, through a smaller window. So the
announcement is stored, then the notice goes out. The same order holds for the shutdown time shown
on a lock screen and for notices. A crash in between then costs at most one notice that never went
out, never a later shutdown. (Claude's call, accepted by the owner.)

1. **Shutdown announcements** (`announced`), per boot. When the engine records an announcement, it
   stores the boot id, the rule window's identity (rule key and window start) and the announcement
   instant. Within one boot the first instant wins, because the shutdown time is computed from it.
   A new boot gets its own row, so an announcement from an earlier boot never blocks the new one.
   On start, the engine fills `announced` from the current boot's rows.
2. **Each lock on screen** (`shown`). Its fields have different lifetimes:
   - `label`, `logical_date`, `test` and `ended` live across boots.
   - `shutdown_on_screen` and `shut_down` are per boot. A lock with a shutdown shuts down again
     after each boot, and that lock's shutdown must not be blocked by an earlier boot's
     `shut_down`.
   - `shutdown_on_screen` changes during a lock, so it is updated in place, not inserted once.

   On start, the engine fills `shown` with every lock not yet ended and with the current boot's
   per-boot fields, which are empty after a new boot.
3. **Notices sent** (`sent`), per boot, keyed by the notice key and its time. On start, the engine
   fills `sent` from the current boot's rows.
4. **Locks that ended while the daemon was down**, across boots. Restoring `shown` (item 2) is
   enough. The first free tick runs the existing `release()`, which records `LockEnded` for every
   lock not yet ended. The recorded time is that tick, not the real end, so after a long downtime or
   a night with the machine off, the report shows the lock ending late. (Claude's call: it reuses
   the existing path, and the usual gap is seconds.)
5. **Rules that already shut the machine down** (`shut_down`), per boot. A repeating rule shuts
   down again after each boot, and a once rule is already blocked across boots by its stored
   `RuleShutdown` event (its key has no timestamp). The builder confirms that last point in
   `selflow-core` before relying on it.

The schema change is a new entry at the end of `MIGRATIONS` in
`crates/selflowd/src/store/migrations.rs`.

## 7. Testing

- Engine restart tests with the existing test clock and an in-memory store shared between two
  `Engine` values, and the boot id injected so a test can keep it or change it:
  - **Restart, same boot id:** tick until a shutdown is announced, drop the engine, build a new one
    on the same store, and check that the shutdown time, the lock screen's shutdown time and the
    notices sent are unchanged. One test restarts in a loop and checks that the shutdown still
    happens on time.
  - **Reboot, new boot id:** a repeating shutdown rule and a lock with a shutdown each give a fresh
    minute of notice and shut down again. A once rule doesn't.
  - **Reboot mid-lock:** a lock that ended while the machine was off gets its `LockEnded` event.
- Codec tests: round trip of every message, the 1 MiB limit, end of stream.
- Handshake tests: matching versions, different versions, and a peer that never answers (the old
  JSON daemon).
- Restart decision tests with a fake restarter and a fake installed binary: restart when the
  installed binary speaks the client's version, no restart when it doesn't, and never a second
  restart in one invocation.
- The byte snapshot test from section 6.1.
- End to end in the isolated environment: an old-protocol daemon, then a new client that restarts
  it. The harness stubs `systemctl`, so the builder either teaches the stub to answer
  `show -p ExecStart` with the harness's copy of `selflowd` and to restart the daemon through the
  harness's own `restart-daemon`, or makes the restarter replaceable for the test.
- A daemon-side handshake test: a connection that sends nothing is closed after 2 seconds.

## 8. Left to the builder

- Names, module layout and the shape of the new tables, within the project's layout rules.
- Whether the engine reads the persisted state only at start or on every tick, as long as it is
  written when it happens.
- How the client waits for the restarted daemon's socket (polling interval and overall timeout).
- The wording of the version mismatch message, through `selflow-i18n` like every other message.
- Re-checking the daemon's one-byte subscriber-close probe, which `6749ee0` already changed, with
  length-delimited frames.
- How `selflowd` parses `--protocol-version` (it takes no arguments today).
- Whether the new bookkeeping rows show up in `selflow report`.

## 9. Open questions for later phases

- **Server sync (M3).** Either sync stays inside `selflowd`, as the main design says (section
  6.1), or it moves to a separate client binary. A separate client would hand the daemon decisions
  made elsewhere (an extension granted on another device, a new config version) that the daemon
  must accept without a challenge. Then the daemon has to verify the message, for example a server
  signature, not the process that relays it. It would also move the device token out of the
  daemon, which section 7.5 protects in the daemon's memory. The transport (gRPC or HTTP) is open
  too.
- **Plugins (phase 4).** If plugins are external processes that talk to the daemon directly, they
  get their own socket that accepts only detector reports, the way PipeWire has `pipewire-0` and
  `pipewire-0-manager` and snapd has `snapd.socket` and `snapd-snap.socket`. Its format can differ
  from the internal one. If plugins run as WASM or through a Selflow binary, this isn't needed.
- **A dedicated system user for the daemon.** Raised by the owner as a question for the future,
  not a plan. Today the user can stop the service with `systemctl --user stop selflowd` and edit or
  delete the SQLite file, which the main design accepts (sections 3 and 8.7). Running `selflowd` as
  a system service under its own `selflow` user would make both need `sudo`, and unlike running it
  as root, a bug in the daemon would stay confined to that account. What it would take:
  - A split into two processes. The `selflow` user can't reach the user's session: the Wayland
    socket and the session bus live in `$XDG_RUNTIME_DIR`, mode 0700 (on the owner's laptop,
    `/run/user/1000`). So a per-session agent running as the user would show the lock screen, send
    notifications and read Hyprland's events, and report to the daemon.
  - A policy for a missing agent. The user can kill the agent, so the daemon would treat its absence
    as a bypass attempt, for example by shutting down after a grace period or by recording the gap
    like the heartbeats of section 8.7.
  - A polkit rule for shutting down. logind's `org.freedesktop.login1.power-off` action is allowed
    without a password only for active sessions (`allow_active=yes`, `auth_admin_keep`
    otherwise), and a system user has no session.
  - An installer that needs `sudo` (the user, a system unit, the polkit rule), and a harder
    isolated test environment.

  It fits the goal of section 8.7, since a bypass would then need the admin password, but it
  changes the architecture and deserves its own design conversation, probably after M3.
