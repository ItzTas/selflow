# Selflow versioned IPC and restart-safe daemon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The local socket carries `postcard` messages behind a versioned handshake, clients
restart an outdated daemon when a restart would fix it, and a daemon restart never changes what a
rule does.

**Architecture:** `selflow-protocol` gets length-delimited `postcard` frames and an 8-byte hello.
`selflow-platform` gets the boot id, the service manager calls and a peer uid check. The daemon
answers the hello, refuses foreign peers and stores its shutdown and notice bookkeeping per boot,
writing each item before acting on it. `selflow-client` checks the installed daemon's version
before restarting it, at most once per process.

**Tech Stack:** Rust 2024, tokio, tokio-util (`LengthDelimitedCodec`), futures-util, serde,
`postcard` 1.2 (new), rusqlite, jiff, thiserror, tracing.

**Spec:** `docs/superpowers/specs/2026-11-26-selflow-ipc-versioning-design.md` (approved
2026-11-26), on top of `docs/superpowers/specs/2026-11-17-selflow-design.md`. The interfaces
below come from the code at `6a88ea3`, not from older plans.

**Repository root:** `/home/talinux/Workspace/github.com/ItzTas/personal-projects/selflow`. Paths
are relative to it.

## Global Constraints

- `CLAUDE.md` rules: English everywhere, no comments in code, `main.rs` holds only `main`,
  `lib.rs` only declares modules, one module per subject with its own `thiserror` enum, OS code
  only in `selflow-platform` or a binary's `src/os/` with one file per system picked by `#[cfg]`,
  none in `selflow-core`, `selflow-i18n` or `selflow-protocol`.
- `make check` passes after every task (fmt, clippy `-D warnings`, tests, the
  `x86_64-pc-windows-msvc` check of core/i18n/protocol, the iced guard). Without a proto-activated
  shell, run cargo as `proto exec rust -- cargo ...`.
- New dependency, checked on crates.io on 2026-11-26: `postcard = { version = "1.2",
  default-features = false, features = ["use-std"] }` (1.2.0, repository active). No other new
  dependency. Own uid comes from the socket file's metadata, not from `libc` (`unsafe_code` is
  forbidden).
- `PROTOCOL_VERSION: u32 = 1`. Hello = `b"SLFW"` + version as little-endian `u32`. Handshake
  timeout 2 s on both sides. Frame limit `MAX_FRAME_BYTES = 1 << 20`.
- Restart command `systemctl --user --no-block restart selflowd.service`; installed binary from the
  `path=` field of `systemctl --user show selflowd.service -p ExecStart --value`. Never the `PATH`,
  never a sibling guess.
- Per-boot state: announcements, notices sent, rules that shut down, and each lock's
  `shutdown_on_screen` and `shut_down`. Across boots: each lock's `label`, `logical_date`, `test`,
  `ended`. Boot id on Linux: `/proc/sys/kernel/random/boot_id`.
- Write before acting. A failed write is logged with `tracing::error!` and the action still
  happens (enforcement never stops because the store failed).
- Tests describe current behaviour (no ghost tests). Tests that change meaning are rewritten, not
  kept beside new ones.
- Commits: Conventional Commits, staged by path, on `main` (the owner's standing branch). No
  co-author lines.

## Review Focus

1. **An old JSON daemon still runs when the new client connects** (the first update). It never
   answers the hello; the client must report a mismatch after 2 s, not hang. Test in Task 2.
2. **A peer sends part of the hello and goes quiet.** The daemon closes the connection after 2 s
   and keeps serving others. Test in Task 2.
3. **The installed daemon predates `--protocol-version`, or the unit is missing.** The old binary
   ignores the flag and exits with `AlreadyRunning`; `systemctl show` may print nothing or no
   `path=`. Each case means "no restart" and a clear error. Tests in Task 6.
4. **The store can't be written** (read-only file, full disk). Restored state stays in memory,
   errors are logged, and locks and shutdowns still happen. Test in Task 5.
5. **The machine reboots in the middle of a lock with a shutdown.** The next boot gives a fresh
   minute and shuts down again, and the lock's end is still recorded once it ends. Test in Task 5.

## Agent safety

Binding on the controller and every subagent.

1. End-to-end runs go only through the isolated environment skill
   (`.claude/skills/testing-selflow-in-isolation/`), authorized by the owner on 2026-11-26. Outside
   it: never run `selflow` (except `--help`), `selflowd`, `selflow-ui`, `systemctl`, `make install`
   or `cargo install`, and never write outside the repository and `mktemp -d` directories.
2. No test may reach the real service manager or the installed binaries. Client tests pass a fake
   `DaemonService` (Task 6). Platform tests call only the pure parser, never `systemctl`.
3. Other sessions work in this tree. Stage by path, never `git add -A`/`.`/`commit -a`, never
   revert or reformat changes you didn't make, re-check `git status` before each commit.

## Order

Run the tasks one at a time in number order. Each one changes code the next one reads. Between
Tasks 1 and 2 the wire format has no handshake, and before Task 6 a mismatch is only reported.
Nobody installs an intermediate commit.

| Task | Needs | Design sections |
|---|---|---|
| 1. Wire format | | 6.1 |
| 2. Handshake | 1 | 6.2, 6.3 (mismatch error only) |
| 3. Peer uid check | 2 | 6.4 |
| 4. Boot id and per-boot store | | 6.5 |
| 5. Engine restores and writes first | 4 | 6.5, 7 |
| 6. Automatic restart | 2, 5 | 6.3 |
| 7. Isolated environment, README, end to end | all | 7 |

---

### Task 1: Wire format (`postcard` in length-delimited frames)

**Files:**
- Modify: `Cargo.toml` (workspace dependency `postcard`)
- Modify: `crates/selflow-protocol/Cargo.toml` (add `postcard`, drop `serde_json`)
- Modify: `crates/selflow-protocol/src/ipc/codec.rs`, `crates/selflow-protocol/src/ipc/mod.rs`
- Create: `crates/selflow-protocol/src/ipc/constants.rs`
- Modify: `crates/selflow-protocol/src/ipc/types.rs` (drop the three `tag` attributes and
  `#[serde(default)]` on `RequestExtension::source`)
- Rewrite: `crates/selflow-protocol/tests/ipc.rs`
- Create: `crates/selflow-protocol/tests/wire/v1.hex`

**Interfaces:**
- Produces (same names as today, so callers keep compiling):
  `pub const MAX_FRAME_BYTES: usize` (replaces `MAX_LINE_BYTES`),
  `pub type MessageReader<R> = FramedRead<R, LengthDelimitedCodec>`,
  `pub fn message_reader<R: AsyncRead>(reader: R) -> MessageReader<R>`,
  `pub async fn write_message<T: Serialize>(writer: &mut (impl AsyncWrite + Unpin), message: &T) -> io::Result<()>`,
  `pub async fn read_message<T: DeserializeOwned>(reader: &mut MessageReader<impl AsyncRead + Unpin>) -> io::Result<Option<T>>`.

- [ ] **Step 1: Rewrite the protocol tests**

Replace the JSON-specific tests (`tags_messages_with_a_snake_case_type`,
`an_extension_request_without_a_source_comes_from_the_screen`,
`rejects_malformed_json_and_reads_none_at_end_of_input`, `round_trips_messages_as_json_lines`,
`rejects_a_line_over_the_limit`) with:
- `round_trips_every_request_and_response`: a `samples()` helper builds one value of every
  `Request` and every `Response` variant (reuse the values the current round-trip tests use);
  write all to a `tokio::io::duplex` pair, read back, `assert_eq!` each.
- `reads_none_at_end_of_input`: empty input gives `Ok(None)`.
- `rejects_a_frame_over_the_limit`: a 4-byte big-endian length of `MAX_FRAME_BYTES + 1` gives
  `ErrorKind::InvalidData`.
- `rejects_bytes_that_are_not_a_message`: a frame holding `[0xff; 8]` gives `InvalidData`.
- `changing_the_wire_format_needs_a_new_protocol_version`: hex of each sample's `postcard`
  bytes, one per line, `assert_eq!(encoded_hex(), include_str!("wire/v1.hex"))`. Task 2 turns it
  into `assert_eq!((PROTOCOL_VERSION, encoded_hex().as_str()), (1, include_str!("wire/v1.hex")))`,
  so whoever updates the bytes sees the version in the same line.

Keep the existing round-trip tests for refusals, test locks, saves and exceptions, moved onto
the new codec.

- [ ] **Step 2: Run them, expect failures**

Run: `cargo test -p selflow-protocol`
Expected: FAIL (compile errors on `MAX_FRAME_BYTES`, then JSON bytes where frames are expected).

- [ ] **Step 3: Implement the codec**

`LengthDelimitedCodec::builder().max_frame_length(MAX_FRAME_BYTES).new_codec()` for both
directions; encode with `postcard::to_stdvec`, send as `tokio_util::bytes::Bytes`; decode with
`postcard::from_bytes`, mapping its error to `InvalidData`. Remove the `tag` attributes and the
`default` attribute in `types.rs`. Generate `wire/v1.hex` from the encoder once and check one line
by hand against the `postcard` wire spec (variant index varint, then fields in order).

- [ ] **Step 4: Fix every caller of `MAX_LINE_BYTES`, run the workspace**

Run: `make check`
Expected: PASS. The daemon, client and test fakes compile unchanged apart from the constant.
`crates/selflow-platform/tests/transport.rs` moves raw bytes and is unaffected.

- [ ] **Step 5: Commit**

`chore: encode IPC messages with postcard in length-delimited frames`

---

### Task 2: Versioned handshake and the mismatch error

**Files:**
- Create: `crates/selflow-protocol/src/ipc/handshake.rs`
- Modify: `crates/selflow-protocol/src/ipc/constants.rs`, `mod.rs`, `tests/ipc.rs`
- Modify: `crates/selflowd/src/ipc/mod.rs` (`serve`, `converse`), `crates/selflowd/src/main.rs`
- Modify: `crates/selflow-client/src/client/mod.rs`, `client/types.rs`, `tests/client.rs`
- Modify: `crates/selflow/src/cli/types.rs`, `crates/selflow/src/cli/mod.rs`,
  `locales/en/cli.ftl`, `locales/pt-BR/cli.ftl`
- Modify test fakes: `crates/selflowd/tests/support/mod.rs` (`TestClient`, `Subscriber`,
  `launch`), `crates/selflow/tests/support/mod.rs` (`FakeDaemon`)
- Test: `crates/selflowd/tests/ipc.rs`

**Interfaces:**
- Consumes: Task 1's codec.
- Produces in `selflow_protocol::ipc`:
  `pub const PROTOCOL_VERSION: u32 = 1`, `pub const HELLO_MAGIC: [u8; 4] = *b"SLFW"`,
  `pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2)`,
  `pub async fn hello_as_client(stream: &mut (impl AsyncRead + AsyncWrite + Unpin)) -> io::Result<u32>`
  (sends our hello, returns the daemon's version),
  `pub async fn hello_as_daemon(stream: &mut (impl AsyncRead + AsyncWrite + Unpin)) -> io::Result<u32>`
  (reads the client's hello, answers with ours, returns the client's version). A wrong magic is
  `InvalidData`. Neither applies the timeout; callers wrap them in `tokio::time::timeout`.
- Produces in `selflow_client::client`:
  `ClientError::VersionMismatch { running: Option<u32>, installed: Option<u32> }` (`running` is
  `None` when the daemon never answered; `installed` stays `None` until Task 6).
- Produces in `selflow`: `CliError::VersionMismatch { running: Option<u32>, installed: Option<u32> }`,
  message id `cli-version-mismatch`, exit code 2.

- [ ] **Step 1: Write the failing tests**

Protocol (`tests/ipc.rs`):
- `a_hello_round_trip_returns_each_sides_version`: over `duplex`, `hello_as_client` returns 1 and
  `hello_as_daemon` returns 1.
- `a_hello_with_another_magic_is_refused`: the daemon side reading `b"JSON\x01\0\0\0"` gives
  `InvalidData`.
- the snapshot test now asserts `(PROTOCOL_VERSION, encoded_hex())`.

Daemon (`crates/selflowd/tests/ipc.rs`, through the support's `launch`):
- `a_client_on_another_version_is_answered_then_closed`: send `SLFW` + `2u32`, read back 8 bytes
  holding version 1, then the next read returns 0 bytes.
- `a_silent_connection_is_closed_after_the_handshake_timeout`: connect, send nothing; within 3 s
  a read returns 0 bytes.
- `a_partial_hello_is_closed_after_the_handshake_timeout`: send `b"SLF"`, same expectation.
- `a_subscriber_stays_open_past_the_handshake_timeout`: after a full hello and `SubscribeLock`,
  sleep 2.5 s, then a view change still arrives.

Client (`tests/client.rs`):
- `a_daemon_that_never_answers_the_hello_is_a_mismatch`: a listener that accepts and reads
  forever; `exchange` returns `VersionMismatch { running: None, installed: None }` within 3 s.
- `a_daemon_on_another_version_is_a_mismatch`: a fake that answers with version 999 gives
  `VersionMismatch { running: Some(999), installed: None }`.

CLI (`crates/selflow/tests/render.rs`): `the_version_mismatch_message_names_each_version`:
render `cli-version-mismatch` with
`client = "1"`, `running = "?"`, `installed = "2"` in both languages; each output contains `1`,
`?` and `2`.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow-protocol -p selflowd -p selflow-client -p selflow`
Expected: FAIL (missing functions and variants).

- [ ] **Step 3: Implement**

- Protocol: `handshake.rs` as in Interfaces; constants in `constants.rs`.
- Daemon: in `converse`, before wrapping in `message_reader`, run
  `timeout(HANDSHAKE_TIMEOUT, hello_as_daemon(&mut stream))`; on timeout, error, or a version other
  than `PROTOCOL_VERSION`, log at `debug` and return (the hello was already answered).
- `selflowd --protocol-version`: in `main`, before tracing starts, if the only argument is
  `--protocol-version`, print `PROTOCOL_VERSION` on stdout and return success. It must not bind the
  socket or open the store.
- Client: `open()` connects, runs `timeout(HANDSHAKE_TIMEOUT, hello_as_client(..))`, and returns
  `VersionMismatch` on timeout or a different version, before `message_reader`.
- Test fakes: `TestClient`/`Subscriber` call `hello_as_client`; `FakeDaemon` calls
  `hello_as_daemon` on each accepted stream.
- CLI copy (exact):
  - `en`: `cli-version-mismatch = The Selflow programs don't match. This command uses protocol { $client }, the running daemon uses { $running } and the installed one uses { $installed }. Reinstall Selflow to fix it: make install`
  - `pt-BR`: `cli-version-mismatch = Os programas do Selflow não combinam. Este comando usa o protocolo { $client }, o daemon em execução usa { $running } e o instalado usa { $installed }. Reinstale o Selflow para corrigir: make install`
  - An unknown version renders as `?`.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`chore: open every IPC connection with a versioned handshake`

---

### Task 3: Refuse peers from other users

**Files:**
- Modify: `crates/selflow-platform/src/transport/types.rs`, `transport/unix.rs`, `transport/mod.rs`
- Modify: `crates/selflowd/src/ipc/mod.rs` (`serve`)
- Test: `crates/selflow-platform/tests/transport.rs`

**Interfaces:**
- Produces: `pub enum AcceptError { Io(#[from] io::Error), ForeignPeer { uid: u32 } }` (thiserror,
  in `transport/types.rs`), `Listener::accept(&self) -> Result<Stream, AcceptError>`, and
  `pub fn check_peer(owner: u32, peer: u32) -> Result<(), AcceptError>`.

- [ ] **Step 1: Write the failing tests**

- `a_peer_of_the_same_user_is_accepted`: listen on a temp socket, connect, `accept` returns `Ok`.
- `check_peer` cases: `check_peer(1000, 1000)` is `Ok`; `check_peer(1000, 1001)` is
  `Err(ForeignPeer { uid: 1001 })`.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow-platform`
Expected: FAIL (`AcceptError`, `check_peer` missing).

- [ ] **Step 3: Implement**

`listen` reads `std::fs::metadata(path)?.uid()` right after `bind` and keeps it as the listener's
`owner`. `accept` reads `inner.peer_cred()?.uid()` and returns `check_peer(owner, uid)` before the
`Stream`. In the daemon's `serve`, `ForeignPeer { uid }` logs `tracing::warn!(uid, "refused a
connection from another user")` and continues the loop; `Io` still ends `serve` as today.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`chore: refuse IPC connections from other users`

---

### Task 4: Boot id and the per-boot store

**Files:**
- Create: `crates/selflow-platform/src/boot/mod.rs`, `boot/unix.rs`; modify `src/lib.rs`
- Modify: `crates/selflowd/src/store/migrations.rs`, `store/mod.rs`, `store/types.rs`
- Test: `crates/selflowd/tests/store.rs`, `crates/selflow-platform/tests/boot.rs`

**Interfaces:**
- Produces in `selflow_platform::boot`: `pub fn boot_id() -> io::Result<String>` (trimmed).
- Produces in `selflowd::store`:
  ```rust
  pub struct Announcement { pub rule_key: String, pub starts_at: Timestamp, pub announced_at: Timestamp }
  pub struct StoredLock {
      pub identity: String, pub label: String, pub logical_date: civil::Date, pub test: bool,
      pub ended: bool, pub shutdown_on_screen: Option<Timestamp>, pub shut_down: bool,
  }
  pub struct BootState {
      pub announced: Vec<Announcement>, pub sent: Vec<(String, Timestamp)>,
      pub shut_down: Vec<String>, pub locks: Vec<StoredLock>,
  }
  impl Store {
      pub fn boot_state(&self, boot_id: &str, since: civil::Date) -> Result<BootState, StoreError>;
      pub fn forget_other_boots(&self, boot_id: &str, since: civil::Date) -> Result<(), StoreError>;
      pub fn record_announcement(&self, boot_id: &str, announcement: &Announcement) -> Result<(), StoreError>;
      pub fn record_notice_sent(&self, boot_id: &str, key: &str, at: Timestamp) -> Result<(), StoreError>;
      pub fn record_shutdown_done(&self, boot_id: &str, key: &str) -> Result<(), StoreError>;
      pub fn save_lock(&self, boot_id: &str, lock: &StoredLock) -> Result<(), StoreError>;
  }
  ```

Migration 4, appended to `MIGRATIONS` (exact):

```sql
CREATE TABLE boot_announcements (boot_id TEXT NOT NULL, rule_key TEXT NOT NULL, starts_at TEXT NOT NULL,
  announced_at TEXT NOT NULL, PRIMARY KEY (boot_id, rule_key, starts_at)) STRICT;
CREATE TABLE boot_notices (boot_id TEXT NOT NULL, key TEXT NOT NULL, at TEXT NOT NULL,
  PRIMARY KEY (boot_id, key, at)) STRICT;
CREATE TABLE boot_shutdowns (boot_id TEXT NOT NULL, key TEXT NOT NULL, PRIMARY KEY (boot_id, key)) STRICT;
CREATE TABLE shown_locks (identity TEXT PRIMARY KEY, label TEXT NOT NULL, logical_date TEXT NOT NULL,
  test INTEGER NOT NULL, ended INTEGER NOT NULL, boot_id TEXT NOT NULL, shutdown_on_screen TEXT,
  shut_down INTEGER NOT NULL) STRICT;
```

- [ ] **Step 1: Write the failing tests**

Store:
- `an_announcement_keeps_its_first_instant_within_a_boot`: record the same rule key and start
  twice with 02:00 then 02:00:30; `boot_state("b1", ..)` has one announcement at 02:00.
- `per_boot_rows_stay_with_their_boot`: rows written under `b1`; `boot_state("b2", ..)` has no
  announcements, notices or shutdowns.
- `a_lock_keeps_its_end_state_across_boots_but_not_its_shutdown`: `save_lock("b1", ..)` with
  `ended = false`, `shutdown_on_screen = Some(23:30)`, `shut_down = true`; `boot_state("b2", ..)`
  returns that lock with `ended = false`, `shutdown_on_screen = None`, `shut_down = false`;
  `boot_state("b1", ..)` returns it unchanged.
- `saving_a_lock_again_updates_it`: second `save_lock` with `shutdown_on_screen = Some(23:31)`
  replaces the first.
- `forgetting_other_boots_keeps_open_locks`: after rows in `b1` and `b2`,
  `forget_other_boots("b2", since)` removes `b1`'s announcements, notices and shutdowns, keeps a
  `b1` lock with `ended = false`, and removes an ended lock whose `logical_date` is before `since`.

Platform: `the_boot_id_is_stable_within_a_boot`: two calls return the same non-empty string.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflowd --test store && cargo test -p selflow-platform --test boot`
Expected: FAIL.

- [ ] **Step 3: Implement**

Announcements and notices use `INSERT OR IGNORE`; `save_lock` uses `INSERT ... ON CONFLICT
(identity) DO UPDATE`. `boot_state` returns locks with `ended = 0` or `logical_date >= since`, and
clears `shutdown_on_screen`/`shut_down` on rows whose `boot_id` differs. Timestamps go through the
store's existing `timestamp_text`.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`chore: store per-boot enforcement bookkeeping`

---

### Task 5: The engine restores its state and writes before acting

**Files:**
- Modify: `crates/selflowd/src/engine/mod.rs`, `engine/types.rs`, `crates/selflowd/src/app.rs`
- Modify: `crates/selflowd/tests/support/mod.rs` (`engine_on`, new `engine_on_boot`, `BOOT`; move
  `second` and `steps` here from `tests/engine.rs` so `tests/restart.rs` can use them)
- Create: `crates/selflowd/tests/restart.rs`
- Modify: `crates/selflowd/tests/engine.rs` (two reboot tests)

**Interfaces:**
- Consumes: Task 4's store API and `boot_id()`.
- Produces: `Engine::new(clock: Arc<C>, enforcer: E, store: Arc<Mutex<Store>>, boot_id: String) -> Result<Self, EngineError>`.
  Test support: `pub const BOOT: &str = "boot-1"`, `engine_on(store, clock)` uses `BOOT`,
  `pub fn engine_on_boot(store: Arc<Mutex<Store>>, clock: Arc<TestClock>, boot: &str) -> (TestEngine, Calls)`.

- [ ] **Step 1: Write the failing tests in `tests/restart.rs`**

Times use the support's `at`, `second`, `setup` (VALID: bedtime 23:00, `shutdown_after = "30m"`,
notices 15/5/1 min) and `start_engine_with(LIGHTS_OUT, ..)` (shutdown rule 02:00 to 04:00).

- `a_restart_keeps_the_announced_shutdown_time`: LIGHTS_OUT at 20 02:00, tick (warns for
  02:01); `engine_on` at 20 02:00:30, tick returns `at(20, 2, 1)` and `steps` is empty; at 02:01
  `steps == ["shut down"]`.
- `restarting_in_a_loop_never_postpones_the_shutdown`: same start, new engines at 02:00:20 and
  02:00:40 each tick with no steps; a new engine at 02:01 shuts down. One warning in total.
- `a_restart_keeps_the_shutdown_time_on_the_lock_screen`: `setup(at(19, 23, 0))`, tick (view
  `shutdown_at` 23:30); `engine_on` at 23:29:30, tick: `last_view().shutdown_at ==
  Some(at(19, 23, 30))`; at 23:30 `steps == ["shut down"]`.
- `a_restart_does_not_repeat_a_notice`: `setup(at(19, 22, 45))`, tick (one warning);
  `engine_on` at 22:46, tick: no `Call::Warn`.
- `a_lock_that_ended_while_the_machine_was_off_gets_its_end`: `setup(at(19, 23, 10))`, tick;
  `engine_on_boot(.., "boot-2")` at `at(20, 7, 30)`, tick: `events_for(date(2026, 11, 19))` holds
  exactly one `LockEnded`.
- `a_reboot_gives_a_lock_with_a_shutdown_a_fresh_minute`: `setup(at(19, 23, 0))`; at 23:30 shuts
  down; `engine_on_boot(.., "boot-2")` at 23:35: view `shutdown_at == Some(at(19, 23, 36))`; at
  23:36 `steps == ["shut down"]`.
- `the_announcement_is_stored_before_the_notice_goes_out`: a `ProbingEnforcer` defined in this
  file holds the store; its `warn` records whether `boot_state(BOOT, ..)` already holds the
  LIGHTS_OUT announcement. Tick at 02:00: the probe saw `true`.
- `a_store_that_cannot_record_still_shuts_down_on_time`: LIGHTS_OUT on a store file made
  read-only (as `keeps_enforcing_when_the_store_cannot_record` does) after the save: the engine
  builds, warns at 02:00 and shuts down at 02:01.

In `tests/engine.rs`, rewrite `once_stays_up_after_a_restart` as `once_stays_up_after_a_reboot`
and `repeat_shuts_down_again_after_a_restart_until_the_rule_ends` as
`repeat_shuts_down_again_after_each_reboot_until_the_rule_ends`, building each later engine with
`engine_on_boot` and a new boot id (`"boot-2"`, `"boot-3"`); assertions stay as they are. The
`a_resume_*` tests (suspend and resume, same engine) don't change.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflowd --test restart --test engine`
Expected: FAIL. The first four restart tests fail on today's code (re-announce, later screen
time, repeated notice, missing `LockEnded`), which is the bug they pin.

- [ ] **Step 3: Implement**

- `Engine::new`: compute `since` as the civil date two days before `clock.now()`; call
  `forget_other_boots` (log on error) and `boot_state` (log on error, start empty); fill
  `announced`, `sent`, `shut_down` and `shown` from it.
- Write before acting, each write logged on failure:
  - `send_notices`: `record_notice_sent` before each `warn`; for a visible shutdown notice,
    `record_announcement` before its `warn`.
  - `announced_shutdown`: `record_announcement` before `warn`.
  - `enforce`: `save_lock` for the entry (with the new `shutdown_on_screen`) and
    `record_announcement` for a rule shown on the lock screen, before `show_lock`; when it decides
    to shut down, `save_lock` with `shut_down = true` and `record_shutdown_done` for the rule,
    before returning.
  - `free`: `record_shutdown_done` before returning `shut_down: true`.
  - `release`: `save_lock` with `ended = true` for each lock it ends.
- `app.rs`: `boot_id()`; on error, `tracing::warn!` and use `format!("unknown-{}",
  Timestamp::now())`, which behaves like a fresh boot on every start, as today.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS, including `keeps_enforcing_when_the_store_cannot_record`.

- [ ] **Step 5: Commit**

`fix: keep shutdown times and notices across daemon restarts`

---

### Task 6: Restart an outdated daemon

**Files:**
- Create: `crates/selflow-platform/src/service/{mod.rs,types.rs,constants.rs,unix.rs}`; modify
  `src/lib.rs`
- Create: `crates/selflow-client/src/client/restart.rs`; modify `client/mod.rs`, `client/types.rs`
- Modify: `crates/selflow-ui/src/lock/mod.rs` (exit on an unfixable mismatch)
- Modify: `crates/selflow/tests/support/mod.rs` (`FakeDaemon::client` uses `connect_with`)
- Test: `crates/selflow-platform/tests/service.rs`, `crates/selflow-client/tests/client.rs`,
  a `selflow-ui` unit test next to the lock code

**Interfaces:**
- Produces in `selflow_platform::service`: `pub const DAEMON_UNIT: &str = "selflowd.service"`,
  `pub enum ServiceError { Io(#[from] io::Error), NoUnit, Failed(ExitStatus) }`,
  `pub fn parse_exec_start(output: &str) -> Option<PathBuf>`,
  `pub fn daemon_binary() -> Result<PathBuf, ServiceError>`,
  `pub fn restart_daemon() -> Result<(), ServiceError>`.
- Produces in `selflow_client::client`:
  ```rust
  pub trait DaemonService: Send + Sync {
      fn installed_version(&self) -> Option<u32>;
      fn restart(&self) -> bool;
  }
  pub struct RestartOnce<S> { /* inner: S, used: AtomicBool */ }
  impl<S: DaemonService> RestartOnce<S> { pub const fn new(inner: S) -> Self; }
  impl<S: DaemonService> DaemonService for RestartOnce<S> { /* restart() calls inner once, then false */ }
  pub struct Systemd;
  impl DaemonService for Systemd { /* through selflow_platform::service */ }
  pub static SYSTEM_SERVICE: RestartOnce<Systemd>;
  impl Client { pub fn connect_with(path: &Path, service: &dyn DaemonService) -> Result<Client, ClientError>; }
  impl LockFeed { pub async fn open_with(path: &Path, service: &dyn DaemonService) -> Result<LockFeed, ClientError>; }
  pub async fn exchange_with(path: &Path, request: &Request, service: &dyn DaemonService) -> Result<Response, ClientError>;
  ```
  `connect`, `open` and `exchange` keep their signatures and pass `&SYSTEM_SERVICE`.
  `ClientError::VersionMismatch` now fills `installed`.

- [ ] **Step 1: Write the failing tests**

Platform (`tests/service.rs`, parser only):
- `the_path_comes_from_exec_start`: the owner's real output
  `{ path=/home/talinux/.cargo/bin/selflowd ; argv[]=/home/talinux/.cargo/bin/selflowd ; ignore_errors=no ; start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }`
  gives `Some("/home/talinux/.cargo/bin/selflowd")`.
- `no_exec_start_means_no_path`: `""`, `"\n"` and `"{ argv[]=/x ; }"` give `None`.

Client (`tests/client.rs`), with a `FakeService { installed: Option<u32>, restarts: AtomicU32 }`
whose `restart` flips the fake daemon's advertised version to `PROTOCOL_VERSION`:
- `an_outdated_daemon_is_restarted_when_the_installed_one_matches`: daemon on 999, installed
  `Some(PROTOCOL_VERSION)`: the request succeeds and `restarts == 1`.
- `no_restart_when_the_installed_daemon_does_not_match`: installed `Some(999)`:
  `VersionMismatch { running: Some(999), installed: Some(999) }`, `restarts == 0`.
- `no_restart_when_the_installed_version_is_unknown`: installed `None` (binary without the flag,
  or no unit): `VersionMismatch { installed: None, .. }`, `restarts == 0`.
- `a_process_restarts_the_daemon_at_most_once`: `RestartOnce::new(FakeService{..})`; first
  `restart()` is `true`, second is `false`, the inner fake counted 1.

`selflow-ui`: `only_a_version_mismatch_ends_the_lock_client`: `lock_exit_code(&VersionMismatch{..})
== Some(3)`, `lock_exit_code(&ClientError::Unreachable) == None`.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow-platform --test service -p selflow-client -p selflow-ui`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Platform `unix.rs`: `daemon_binary` runs `systemctl --user show selflowd.service -p ExecStart
  --value` and parses it (`NoUnit` when `parse_exec_start` is `None`); `restart_daemon` runs
  `systemctl --user --no-block restart selflowd.service`.
- `Systemd::installed_version`: `daemon_binary()`, run it with `--protocol-version`, kill it if it
  hasn't exited within `HANDSHAKE_TIMEOUT`, and return the parsed stdout only on success.
  Anything else is `None` (Review Focus 3: an old binary ignores the flag and exits with
  `AlreadyRunning`).
- Glue in `restart.rs`: on a mismatch, `installed_version()`; when it equals `PROTOCOL_VERSION`
  and `restart()` is `true`, poll `connect` every 100 ms for up to 10 s, then one more handshake;
  otherwise return `VersionMismatch { running, installed }`.
- `selflow-ui` lock: `fn lock_exit_code(error: &ClientError) -> Option<i32>`; in the reconnect
  loop, `Some(code)` logs both versions at `error` and exits the process with it. The supervisor's
  back-off restarts it (design 6.3).

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`feat: restart an outdated daemon automatically`

---

### Task 7: Isolated environment, README and the end-to-end check

**Files:**
- Modify: `.claude/skills/testing-selflow-in-isolation/scripts/isolated-env.sh` (`write_stubs`)
- Modify: `.claude/skills/testing-selflow-in-isolation/SKILL.md` (one line on the new stub)
- Modify: `README.md` (the update paragraph, line 31 today)

- [ ] **Step 1: Teach the `systemctl` stub two calls**

`systemctl --user show selflowd.service -p ExecStart --value` prints
`{ path=$DIR/bin/selflowd ; argv[]=$DIR/bin/selflowd ; }`, and
`systemctl --user --no-block restart selflowd.service` runs the harness's own `restart-daemon` for
`$DIR` in the background. Every other call stays blocked and logged as today.

- [ ] **Step 2: Run the scenario in the isolated environment (level 1)**

Build the old JSON daemon with `cargo build -p selflowd` in a `mktemp -d` clone checked out at
`6a88ea3`. Never touch the shared working tree's checkout.

1. **Update case.** Start the isolated environment, put the old `selflowd` into `$DIR/bin`, run
   `bash $ISO restart-daemon $DIR`, then put the current `selflowd` back into `$DIR/bin` and run
   `bash $ISO run $DIR selflow config status`.
   Expected: the command answers normally; `daemon.log` shows one stop and one start;
   `blocked.log` shows no other `systemctl` call.
2. **Partial install case.** Put the old `selflowd` into `$DIR/bin` again, run
   `bash $ISO restart-daemon $DIR`, and run the same command.
   Expected: the `cli-version-mismatch` text with client `1`, running `?` and installed `?` (the
   old binary ignores `--protocol-version`, hits `AlreadyRunning` and exits), and no restart in
   `daemon.log`.

- [ ] **Step 3: Measure the idle daemon**

Run: `bash $ISO measure-idle $DIR 10`
Expected: CPU close to 0% and RSS in the low tens of MB (main design §15), compared with the
number in the M2.1 report.

- [ ] **Step 4: Update the README**

Replace the update paragraph: after `make install`, the next `selflow` command restarts the
daemon when the new one is installed; if the installed programs don't match, the command says so.
Run the owner's humanizer pass on that paragraph (user-facing docs).

- [ ] **Step 5: Run and commit**

Run: `make check`
Expected: PASS.

`chore: let the isolated environment restart the daemon` (stub and skill) and
`docs: describe updating Selflow with the automatic restart` (README), as two commits.
