# Selflow M2.1 "Rules and steps" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On one machine, a rule can send notices, lock the screen, shut the machine down with or
without a lock, or any mix of these, each rule with its own text, challenges and extension budget.
Each change to the configuration takes effect on its own clock, a lock can be lifted from a
terminal, exception days skip rules, and the command line no longer links the graphics stack.

**Architecture:** M1's crates plus three new ones. `selflow-platform` holds the operating system
code the binaries share, `selflow-client` holds the code that talks to the daemon, and the
`selflow-ui` binary holds the lock screen (the tray and the window join it in M2.2).
`selflow-core` still decides everything as pure functions. A rule now has an action (nothing, a
lock or a shutdown), notices, a message and its own challenges. `versions` splits every save into
units, gives each unit its own clock, and rebuilds the configuration in force at any moment from
them. `enforce` evaluates notices, locks and shutdowns for every rule. The daemon stores each save
together with the time each of its units takes effect.

**Tech Stack:** as in M1: Rust 2024, tokio, serde and toml, jiff 1.0, fastrand, thiserror,
tracing, fluent-bundle, rusqlite (bundled), serde_json, clap, notify-rust, tempfile, iced 0.14,
iced_exwlshell 0.21. No new third-party dependency.

**Spec:** `docs/superpowers/specs/2026-11-17-selflow-design.md`, as updated on 2026-11-20. This
plan is the first half of milestone M2 in `docs/superpowers/plans/2026-11-18-selflow-phase-1-roadmap.md`
(see "Scope"). What M1 built, and every way it departed from its plan, is in
`.superpowers/sdd/2026-11-18-selflow-m1-night-lock/progress.md` (rulings R0 to R39) and
`task-17-report.md` there (the test-lock command, R40 and R41). The interfaces below come from the
code, not from the M1 plan.

**Repository root:** `/home/talinux/Workspace/github.com/ItzTas/personal-projects/selflow`. Every
path in this plan is relative to it unless it starts with `/` or `~`.

**Status:** not started. The owner settled its open questions on 2026-11-20, and the spec, the
roadmap and M1's Task 16 files are committed (4e1556c, f937a3d, 03a1f80, 7d844c4). Before it
starts, the owner finishes M1's checks and measurement (M1 Task 16, Steps 5 to 7), so a problem
found there is fixed on M1's code, and the spec's §8.9 line on rules with only notices (added on
2026-11-20) is committed. Run it as described in "Executing this plan with subagents", after
reading "Agent safety".

## Scope

The roadmap left it to this plan whether M2 splits in two. It splits (Claude's call, approved by
the owner on 2026-11-20). M2 grew to about twenty tasks that day, the first half already gives the
owner something to use every day, and the second half builds on the rule model the first half
creates, so its plan should be written on the real code, as the roadmap does for every milestone.

| Part | What the owner gets | Plan |
|---|---|---|
| M2.1 Rules and steps | The operating system layer and the `selflow`/`selflow-ui` split; rules with any mix of notices, a lock and a shutdown (with or without a lock, once or on every boot); custom messages; challenges and an extension budget per rule; each change on its own clock; exception days; `selflow unlock`; the M1 items parked for M2 | this file |
| Lock screen themes (§8.5.1) | `~/.config/selflow/lock.toml`, the default theme, `selflow lock theme init` | a small plan of its own, right after M2.1; it needs only `selflow-ui` |
| M2.2 Watching apps | The Hyprland adapter, the process detector, rules by window and process with continuous or added-up time, work classification and the local workday, app blocking and the unblock flow, bounded inhibit, the tray and the small window, events without window titles | written once M2b is built |

The owner set the order after M2.1 on 2026-11-20: the lock screen themes, then M2b (Claude Code
sessions), then M2.2. M2b needs M2.1's rule steps; its steps cover sessions by folder (spec §8.12),
so it doesn't wait for M2.2's work classification.

**Done when** the owner, on the laptop (Task 10): sees the bedtime lock work from the new file
through `selflow-ui`; gets a reminder rule's own text at its time with no lock; sees a shutdown
rule without a lock give its minute of notice, repeat after a restart when it says `repeat` and not
when it says `once` (power in log mode); saves an earlier bedtime and an extra extension together
and sees the bedtime apply at once while `selflow config status` lists the extension with its own
date, a date a second save leaves alone; lifts a lock from a TTY with `selflow unlock`; skips
tonight's lock with `selflow exception --rule bedtime` after its challenges; and `selflow` builds
without iced while `selflow-core`, `selflow-i18n` and `selflow-protocol` build for Windows. The
idle daemon stays under 30 MB of RAM and under 1 s of CPU per 10 minutes.

## Global Constraints

- English for code, identifiers, logs and source copy. Every string a person sees goes through
  `selflow-i18n`, `en` and `pt-BR` stay complete (the parity test enforces it), and copy is calm,
  without guilt or alarm (spec §4). `.ftl` copy and the default configuration files follow that
  tone rule; README prose goes through the owner's humanizer skill (M1 ruling R9).
- Layout from spec §19: `main.rs` holds only `main`, `lib.rs` only declares modules, one module per
  subject (a folder with `mod.rs`, `types.rs`, `constants.rs` as it grows), each subject module
  owns its `thiserror` error enum, CLI commands live in `src/cli/commands/<name>_cmd.rs` with
  `<Name>Args` and `<name>_cmd()`.
- Operating system code (spec §19): what several binaries share lives in `selflow-platform`, what
  one binary needs lives in its `src/os/`, and each holds one file per system (`unix.rs` today),
  picked with `#[cfg]` and re-exported by `mod.rs`. `selflow-core`, `selflow-i18n` and
  `selflow-protocol` hold none, and `make check` builds them for `x86_64-pc-windows-msvc`.
- No comments in the code.
- Only `selflow-ui` depends on `iced`, with `iced_exwlshell` under
  `[target.'cfg(target_os = "linux")'.dependencies]`. `selflowd` and `selflow` never link a GUI
  library (`make check` guards `selflow`). Binary crates never depend on each other; what
  `selflow` and `selflow-ui` share for talking to the daemon lives in `selflow-client`.
- No `unsafe` (workspace lint). Tests never call `std::env::set_var`: code that reads the
  environment has a `*_from` variant, or receives the value through a struct the tests build.
- Paths as in M1: store `$XDG_STATE_HOME/selflow/selflow.db` (else `~/.local/state/selflow/`),
  directory 0700 and file 0600; socket `$XDG_RUNTIME_DIR/selflow/daemon.sock`, directory 0700.
- Defaults and rules from the spec: notices 15, 5 and 1 minute before a rule that locks or shuts
  down, and one notice at the start of any other rule; challenges are a wait, a written reason and
  an extension budget, now per rule with rule-set defaults; a PIN is never a default; loosening
  waits 24 hours; stricter and neutral changes apply at once; each change keeps its own clock; a
  rule that already started, or whose first notice already went out, keeps its times until it
  ends, while stricter changes still reach it; every shutdown comes at least a minute after a
  notice or a lock screen announced it (spec §8.3, §8.5, §8.9).
- Resource targets (spec §15): the idle daemon stays near 0% CPU and in the low tens of MB, and
  nothing in it ticks every second.
- Dependencies: Tasks 1 and 2 add the workspace crates `selflow-platform` and `selflow-client` and
  the binary `selflow-ui`. No task adds a third-party crate. If one turns out to be needed, stop and
  report NEEDS_CONTEXT; the owner's library rules (`~/.claude/rules/libraries.md`) apply.
- Each crate's `tests/support/mod.rs` starts with `#![allow(dead_code)]`.
- `make check` passes at the end of every task. From Task 1 on it runs fmt, clippy with
  `-D warnings`, the tests and the Windows check; from Task 2 on it also guards `selflow` against
  iced.
- Toolchain (M1 ruling R1): run every `cargo`, `make` and `git commit` as
  `proto exec rust -- <command>`, because a plain `cargo` in this shell is rustup's older stable.
  jiff is 1.0.1: `ZonedDateTime`, `to_zoned_datetime`, and `Timestamp - Timestamp` is a
  `SignedDuration` (M1 ruling R12).
- Tests describe what the code does now (owner's test rules, `~/.claude/rules/tests.md`). Rewrite a
  test that a task makes obsolete instead of patching in what changed.
- Commits follow Conventional Commits, one logical change each, staged by path. Only Task 10 uses
  `feat:`; every other task uses `chore:`. Never add `Co-Authored-By` or any other trailer.

## Review Focus

1. Upgrading from M1. The owner's store holds M1 versions in the old format, which no longer
   parse. The daemon must start cleanly, keep the report history, and show the old settings as
   comments under the new default file on the first `selflow config edit` (Task 5,
   `an_m1_store_opens_with_its_settings_kept_aside` and
   `after_an_upgrade_the_first_edit_shows_the_old_settings`).
2. Saves that mix directions, followed by more saves. The tightening applies at once, the
   loosening keeps the date of its first save, putting a value back cancels its wait, and a value
   between the one in force and a waiting one keeps the waiting date (Task 4,
   `a_later_save_does_not_restart_a_waiting_change` and three neighbors).
3. Booting or resuming inside a shutdown without a lock. `repeat` gives a minute of notice and
   shuts down on every start of the daemon until the rule ends, `once` never shuts down twice on
   the same day, and a laptop that resumes after the start still gets its `once` shutdown with a
   minute of notice. The minute counts from when a notice really went out, and during another
   rule's lock, where desktop notices don't show, from when the time appeared on the lock screen
   (Task 7, `a_resume_after_the_start_still_gets_its_one_shutdown`,
   `the_minute_counts_from_when_the_notice_went_out` and
   `a_shutdown_rule_during_a_lock_shows_on_the_lock_screen`).
4. A lock screen that froze or died when the lock ended. `selflow unlock` from another TTY
   frees the screen: through the challenges while a lock is on, and at once when nothing is locked
   (Task 8, `a_client_killed_after_the_lock_ends_is_followed_by_one_release_client` and
   `releasing_with_nothing_locked_starts_one_client`).
5. A same-day exception used as a shortcut. It goes through the rule's challenges, needs an
   extension left for that rule, is refused while that rule's lock is on screen, and shows in the
   report (Task 9, `an_exception_is_refused_while_its_lock_is_on_screen` and neighbors).

## Agent safety

These rules protect the owner's session and machine. They bind the controlling agent and every
subagent.

1. Never run `selflow-ui --lock`, `selflow-ui --preview` or anything that starts them. A lock client
   takes the Wayland session lock at once and only lets go when its daemon says so, so it would
   lock the owner out of the session they are working in. The owner looks at the screen in
   Task 10 (M1 ruling R19).
2. Never run a `selflow` command other than `selflow --help` and `selflow <command> --help`.
   `unlock`, `exception`, `test-lock` and `config` talk to the owner's real daemon through
   `$XDG_RUNTIME_DIR/selflow/daemon.sock`; tests use temporary sockets.
3. When a step runs `selflowd` itself, copy the built binary into a fresh temporary directory, run
   it from there with a `PATH` that holds no `selflow-ui`, set `SELFLOW_POWER=log`, and point
   `XDG_STATE_HOME` and `XDG_RUNTIME_DIR` at fresh temporary directories (M1 ruling R8). Never
   save a configuration whose lock window includes the current time.
4. Lock-supervisor tests spawn stand-in programs (`sleep`, `sh -c ...`), never `selflow-ui`.
5. Never run `systemctl poweroff`, `systemctl --user`, `make install` or `cargo install`, and never
   write outside the repository (dotfiles, `~/.config`, `~/.local`). Temporary directories from
   `mktemp -d` are fine. Never edit the owner's Hyprland configuration.
6. Ask the owner before creating a branch or a git worktree (owner's git rules). M1 ran on `main`.
7. Other sessions work in this repository. Stage files by path, never `git add -A`, `git add .` or
   `git commit -a`, and never revert or reformat changes you didn't make.
8. Tests never call a `*_cmd(args)` function: each of them reads the real socket path. A command's
   test calls its `*_with` seam with a `Client` connected to `FakeDaemon`, a scripted daemon on a
   temporary socket (Task 8).

## Executing this plan with subagents

Use superpowers:subagent-driven-development, as in M1 (the owner's choice on 2026-11-20): one
fresh implementer per task, one fresh reviewer per task, and a review of the whole range at the
end.

**What each implementer receives:** the full text of its task, the Global Constraints, the Agent
safety rules, the paths of this plan and of the spec with the sections listed below, and the
controller's rulings so far. A task's Interfaces block names what it uses from earlier tasks.

**Order.** Run the tasks one at a time in number order. Each one rewrites code the next one reads,
so none of them run in parallel.

**Staging between tasks.** Task 3 changes the configuration format before later tasks put it to
full use. Between Tasks 3 and 6, rules without a lock parse and are judged by `selflow config
edit`, but the daemon doesn't act on them yet; between Tasks 3 and 5 the daemon still gives a
whole save one clock; between Tasks 6 and 7 a shutdown without a lock only sends its notices.
Reviewers should not flag those gaps. Nobody installs an intermediate commit.

| Task | Needs | Spec sections | Test fixtures |
|---|---|---|---|
| 1. Operating system layer | | §6.4, §19 | platform tests use temporary directories |
| 2. Client and `selflow-ui` | 1 | §6.1, §15, §19 | `selflow-ui` tests get `view` and `at` from `selflow`'s support |
| 3. Configuration with steps | 2 | §8.2, §8.3, §8.5, §8.8, §8.9, §8.11, §20 | core and daemon `VALID` move to the new format |
| 4. Each change on its own clock | 3 | §8.9, §20 | adds core `history_of` |
| 5. The daemon on per-change clocks | 4 | §6.7, §8.3, §8.9, §8.11 | adds daemon `save_at` (saves through `plan_save`); `save_text` calls it at 18 10:00 |
| 6. Rules beyond the lock | 5 | §8.2, §8.3, §8.5, §8.9 | adds core `REMINDER` and `TWO_LOCKS` |
| 7. Shutdown without a lock | 6 | §8.3, §8.9 | adds daemon `LIGHTS_OUT` |
| 8. Unlock from a terminal | 7 | §6.4, §8.5, §8.13 | adds selflow `ScriptedPrompt` and `FakeDaemon` |
| 9. Exceptions on the same day | 8 | §8.5, §8.9 | uses `ScriptedPrompt` and `FakeDaemon` |
| 10. Install, owner check, measurement | all | §6.1, §15, §17 | the owner takes part |

---

### Task 1: Operating system layer (`selflow-platform`, `src/os/`)

No behavior changes; M1's tests show it. talinux-10 designed this layer and reviewed these
boundaries on 2026-11-20.

**Files:**
- Create: `crates/selflow-platform/Cargo.toml`, `src/lib.rs`, `src/paths/{mod.rs,unix.rs}`,
  `src/secure_fs/{mod.rs,unix.rs}`, `src/transport/{mod.rs,types.rs,unix.rs}`
- Create: `crates/selflowd/src/os/{mod.rs,unix.rs}`, `crates/selflow/src/os/{mod.rs,unix.rs}`
- Modify: root `Cargo.toml` (workspace dependency `selflow-platform`), `crates/selflowd/Cargo.toml`,
  `crates/selflow/Cargo.toml`, both crates' `src/lib.rs` (`pub mod os;`)
- Modify: `crates/selflow-protocol/src/ipc/{mod.rs,codec.rs}` (socket path removed),
  `crates/selflowd/src/{app.rs,store/mod.rs,ipc/mod.rs,ipc/types.rs,enforcer/power.rs}`,
  `crates/selflow/src/{client/mod.rs,editor/mod.rs,lock/mod.rs}`,
  `crates/selflow/src/cli/commands/{config_cmd.rs,report_cmd.rs,test_lock_cmd.rs,lock_cmd.rs}`
- Modify: `Makefile`, `.github/workflows/ci.yml`, `CLAUDE.md`
- Test: `crates/selflow-platform/tests/{paths.rs,secure_fs.rs,transport.rs}`; remove the moved
  tests from `crates/selflow-protocol/tests/ipc.rs` (`the_socket_lives_under_the_runtime_dir`),
  `crates/selflowd/tests/store.rs` (`the_default_path_follows_xdg_state_home`) and
  `crates/selflowd/tests/ipc.rs` (`a_second_daemon_cannot_take_the_socket`); update
  `crates/selflowd/tests/support/mod.rs` (`launch` listens through `selflow_platform`)

**Interfaces:**
- Produces, in `selflow-platform` (dependencies: tokio, thiserror). Each item lives in `unix.rs`
  under `#[cfg(unix)]` and `mod.rs` re-exports it:
  - `paths::store_path_from(state_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf>`
    (a non-empty `state_home` wins, else `home/.local/state`, then `selflow/selflow.db`) and
    `paths::store_path() -> Option<PathBuf>`.
  - `paths::runtime_dir_from(runtime_dir: Option<&OsStr>) -> Option<PathBuf>` (`<dir>/selflow`) and
    `paths::runtime_dir()`.
  - `paths::socket_path_from(runtime_dir: Option<&OsStr>) -> Option<PathBuf>`
    (`<dir>/selflow/daemon.sock`) and `paths::socket_path()`.
  - `secure_fs::private_dir(path: &Path) -> io::Result<()>`: creates the directory and its
    parents and leaves it at 0700, tightening one that already exists.
  - `secure_fs::private_file(path: &Path) -> io::Result<()>`: creates the file when missing,
    keeps its contents, and leaves it at 0600.
  - `transport::Listener` and `transport::Stream`, newtypes whose public API names no Unix or
    tokio type (talinux-10's review): `async fn accept(&self) -> io::Result<Stream>` on
    `Listener`, and `Stream` implements `tokio::io::AsyncRead + AsyncWrite + Unpin` by
    delegating to the socket inside, so `BufReader<Stream>` works as today.
  - `transport::listen(path: &Path) -> Result<Listener, ListenError>`: `private_dir` on the parent,
    `ListenError::AlreadyRunning` when a connection to `path` succeeds, removal of a leftover file,
    bind (`AddrInUse` also gives `AlreadyRunning`). Call it inside a tokio runtime.
  - `transport::connect(path: &Path)`, an `async fn` returning `io::Result<Stream>`.
  - `#[derive(Debug, thiserror::Error)] pub enum ListenError { AlreadyRunning, Io(#[from] io::Error) }`.
- Produces in `selflowd::os`: `power_off() -> io::Result<()>` (spawns `systemctl poweroff` with
  stdin closed and doesn't wait), and `StopSignal` with `new() -> io::Result<StopSignal>` and
  `async fn recv(&mut self)` (SIGTERM). `enforcer::shut_down(PowerMode)` keeps deciding and calls
  `os::power_off` in `Real`.
- Produces in `selflow::os`: `run_editor(path: &Path) -> io::Result<ExitStatus>`, running
  `sh -c '${VISUAL:-${EDITOR:-vi}} "$1"' sh <path>`.
- Removes: `selflow_protocol::ipc::{socket_path, socket_path_from}`,
  `selflowd::store::{default_path, default_path_from}`, `selflowd::ipc::bind` and
  `IpcError::AlreadyRunning` (`app.rs` maps `ListenError::AlreadyRunning` to
  `AppError::AlreadyRunning`). `Store::open` uses `private_dir` and `private_file`;
  `TempfileEditor` keeps its temporary files in `paths::runtime_dir()`, made with `private_dir`.

- [ ] **Step 1: Write the failing tests** in `crates/selflow-platform/tests/`:

```rust
#[test]
fn the_store_lives_under_xdg_state_home_or_home() {
    assert_eq!(store_path_from(Some("/s".as_ref()), Some("/h".as_ref())), Some("/s/selflow/selflow.db".into()));
    assert_eq!(store_path_from(Some("".as_ref()), Some("/h".as_ref())), Some("/h/.local/state/selflow/selflow.db".into()));
    assert_eq!(store_path_from(None, None), None);
}

#[test]
fn the_socket_lives_under_the_runtime_dir() {
    assert_eq!(socket_path_from(Some("/run/user/1000".as_ref())), Some("/run/user/1000/selflow/daemon.sock".into()));
    assert_eq!(socket_path_from(None), None);
}
```

  `secure_fs.rs`: `a_private_dir_is_created_and_an_existing_one_tightened` (a nested path ends at
  mode 0o700; a directory chmodded to 0o755 first also ends at 0o700) and
  `a_private_file_keeps_its_contents_and_gets_mode_0600` (write "x", chmod 0o644, call it, read
  "x" back and see 0o600). `transport.rs`: `a_line_goes_both_ways` (listen in a temporary
  directory, connect, one side writes `"ping\n"`, the other reads it with `read_line` and answers
  `"pong\n"`), `a_second_listener_is_refused_while_the_first_answers`
  (`Err(ListenError::AlreadyRunning)`) and `a_leftover_socket_file_is_replaced` (a plain file at
  the path, then `listen` succeeds).
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflow-platform`. Expected: FAIL, the crate
  doesn't exist.
- [ ] **Step 3: Create `selflow-platform`** with the Interfaces above, then move every caller to
  it and remove the old functions and the three moved tests.
- [ ] **Step 4: Add `src/os/`** to `selflowd` and `selflow` as above.
- [ ] **Step 5: Run** `proto exec rust -- cargo test --workspace`. Expected: PASS, with every M1
  test unchanged apart from the three that moved.
- [ ] **Step 6: Commit** `chore: add the operating system layer`.
- [ ] **Step 7: The Windows check.** In `Makefile`, add `WINDOWS_TARGET := x86_64-pc-windows-msvc`
  and append to `check` exactly these two lines. The first fails with the command to run when the
  toolchain `rustc` resolves to lacks the target; it reads the sysroot because under
  `proto exec`, `rustup` still reports its own default toolchain (checked on 2026-11-20):

```make
	@test -d "$$(rustc --print sysroot)/lib/rustlib/$(WINDOWS_TARGET)" || { echo "make check needs the $(WINDOWS_TARGET) target: rustup target add --toolchain $$(basename "$$(rustc --print sysroot)") $(WINDOWS_TARGET)" >&2; exit 1; }
	cargo check --target $(WINDOWS_TARGET) -p selflow-core -p selflow-i18n -p selflow-protocol
```

  In `ci.yml`, give `dtolnay/rust-toolchain@stable` the line `targets: x86_64-pc-windows-msvc`. In
  `CLAUDE.md`, extend the layout line with: "operating system code lives in `selflow-platform`
  (shared) or the binary's `src/os/` (its own), one file per system (`unix.rs`) picked with
  `#[cfg]`; `selflow-core`, `selflow-i18n` and `selflow-protocol` hold none, and `make check`
  builds them for Windows."
- [ ] **Step 8: Run** `proto exec rust -- make check`. Expected: exit 0 on the owner's laptop,
  which has the target.
- [ ] **Step 9: Commit** `chore: check the shared crates against a windows target`.

### Task 2: Client crate and `selflow-ui`

**Files:**
- Create: `crates/selflow-client/{Cargo.toml,src/lib.rs,src/client/mod.rs,src/client/types.rs}`,
  `crates/selflow-client/tests/client.rs`
- Create: `crates/selflow-ui/{Cargo.toml,src/main.rs,src/lib.rs,src/app.rs,src/lock/mod.rs}`,
  `crates/selflow-ui/src/lock/backend/{mod.rs,wayland.rs}`,
  `crates/selflow-ui/tests/support/mod.rs`
- Move with `git mv`: `crates/selflow/src/lock/{model.rs,view.rs,style.rs}` to
  `crates/selflow-ui/src/lock/`, and `crates/selflow/tests/lock_model.rs` to `crates/selflow-ui/tests/`
- Delete: `crates/selflow/src/lock/mod.rs`, `crates/selflow/src/client/mod.rs`,
  `crates/selflow/src/cli/commands/lock_cmd.rs`
- Modify: root `Cargo.toml` (workspace dependency `selflow-client`), `crates/selflow/Cargo.toml`
  (iced and iced_exwlshell removed), `crates/selflow/src/{lib.rs,cli/mod.rs,cli/types.rs}`,
  `crates/selflow/src/cli/commands/{mod.rs,config_cmd.rs,report_cmd.rs,test_lock_cmd.rs}`,
  `crates/selflowd/src/{app.rs,enforcer/lock_supervisor.rs}`, `crates/selflowd/tests/lock_supervisor.rs`
- Modify: `Makefile` (`install` also installs `crates/selflow-ui`; `check` gains the iced guard),
  `README.md` (`selflow-ui --preview` replaces `selflow lock --preview`, and the idle-locker guard
  and the stuck-lock note name `selflow-ui --lock` instead of `selflow lock`)

**Interfaces:**
- Consumes: `selflow_platform::{paths::socket_path, transport::{connect, Stream}}` (Task 1).
- Produces in `selflow-client` (dependencies: selflow-protocol, selflow-platform, tokio,
  thiserror). Spec §20 leaves the choice between a new crate and `selflow-protocol` to the
  builder; a new crate keeps the local client out of the messages the server and a future phone
  share, as talinux-10 suggested (Claude's call, approved by the owner on 2026-11-20):
  - `pub struct Client`, blocking, owning a current-thread runtime (M1 ruling R5: never called from
    async code): `connect(path: &Path) -> Result<Client, ClientError>`,
    `request(&mut self, request: Request) -> Result<Response, ClientError>`.
  - `pub async fn exchange(path: &Path, request: &Request) -> Result<Response, ClientError>`: one
    request on a fresh connection, for async callers.
  - `pub struct LockFeed` with `async fn open(path: &Path) -> Result<LockFeed, ClientError>` (sends
    `SubscribeLock`) and `async fn next(&mut self) -> Result<Option<Response>, ClientError>`.
  - `#[derive(Debug, thiserror::Error)] pub enum ClientError { Unreachable, Protocol(#[source] io::Error) }`:
    `Unreachable` when the connection fails or closes before an answer, as M1's client did.
- `selflow`: `CliError` gets `From<ClientError>` (`Unreachable` to `DaemonUnreachable`, `Protocol`
  to `Protocol`); `Command::Lock`, `LockArgs`, `lock_cmd` and `CliError::Lock` go away.
- Produces in `selflow-ui` (dependencies: selflow-i18n, selflow-protocol, selflow-client,
  selflow-platform, clap, iced, tokio, jiff, thiserror, tracing, tracing-subscriber; Linux only:
  iced_exwlshell):
  - Flags `--lock` (the session lock the daemon starts) and `--preview` (the lock screen in a
    window), conflicting, with `arg_required_else_help`. `app::run() -> ExitCode` installs M1's
    WARN-level stderr subscriber (ruling R22) and dispatches; a `LockError` is logged and exits 1.
  - `lock::LockScreen`: M1's iced app (model, view, style, ticks, keyboard), platform neutral. It
    follows the daemon through `LockFeed` and asks for time through `exchange`, with M1's
    reconnect, timeout and failure behavior (rulings R20, R21). `lock::run_preview()`.
  - `lock::backend::run_session_lock(socket: PathBuf) -> Result<(), BackendError>` in
    `backend/wayland.rs` (`#[cfg(target_os = "linux")]`): runs
    `iced_exwlshell::sessionlock::application` over a state holding the `LockScreen` and the shell
    receiver, with its own message enum `SessionMessage { Screen(Message), Lost(ShellEvent) }`
    marked `#[to_sessionlock_message]` (the macro only adds a `TryInto` impl, so wrapping works).
    `LockDenied` and `LockedFinished` exit, as M1 does. `BackendError` wraps
    `iced_exwlshell::Error`.
- `selflowd`: `lock_program_from(current_exe)` returns the sibling `selflow-ui`, else `selflow-ui`
  on `PATH`; `app.rs` starts it with `["--lock"]`.

- [ ] **Step 1: Write the failing tests.** `crates/selflow-client/tests/client.rs`:
  `a_request_gets_its_answer` is a plain `#[test]`. A thread with its own tokio runtime listens
  through `selflow_platform::transport::listen` in a temporary directory, answers one `Status` with
  `Response::LockEnded`, and signals on a `std::sync::mpsc` channel once it listens; the test then
  calls `Client::connect(&path)?.request(Request::Status)` and gets `Response::LockEnded`.
  `connecting_to_a_missing_socket_is_unreachable` expects `Err(ClientError::Unreachable)`.
  `a_lock_feed_reads_what_the_daemon_sends` (`#[tokio::test]`): the fake daemon answers the
  subscription with `Lock(view)` and `LockEnded`, and `next()` returns both, then `Ok(None)`.
  In `crates/selflowd/tests/lock_supervisor.rs`, the `lock_program_from` test expects a sibling
  file named `selflow-ui`, and `selflow-ui` on `PATH` otherwise.
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflow-client -p selflowd --test lock_supervisor`.
  Expected: FAIL.
- [ ] **Step 3: Implement `selflow-client`** and move the CLI onto it.
- [ ] **Step 4: Create `selflow-ui`**, move the lock screen files and their tests (the moved tests
  keep their assertions; `view` and `at` move into `selflow-ui`'s `tests/support`), and point the
  daemon at `selflow-ui --lock`.
- [ ] **Step 5: The iced guard.** Append to `check` in `Makefile`:

```make
	! cargo tree -p selflow -e normal --prefix none | grep -q '^iced'
```

- [ ] **Step 6: Run** `proto exec rust -- make check` and
  `proto exec rust -- cargo tree -p selflow -e normal --prefix none | grep -c '^iced'`.
  Expected: exit 0, and a count of 0. Never run the new binary (Agent safety rule 1).
- [ ] **Step 7: Commit** `chore: share the daemon client between the binaries` (the
  `selflow-client` crate and the CLI on it), then `chore: move the lock screen into selflow-ui`.

### Task 3: Configuration with steps (`selflow-core::config`, change classification)

A rule replaces M1's lock. It fires at its `start` and takes an action: nothing (its notices only),
a lock until its end, or a shutdown. Challenges move to the rule, with the rule set's values as
defaults. Every field's changes are classified here, still as one kind per save; Task 4 gives each
unit its own clock.

**Files:**
- Modify: `crates/selflow-core/src/config/{mod.rs,types.rs,raw.rs,validate.rs,constants.rs}`,
  `crates/selflow-core/src/timing/mod.rs`, `crates/selflow-core/src/versions/{mod.rs,types.rs,compare.rs}`
- Modify, porting to the new types only: `crates/selflow-core/src/plan/mod.rs` (windows for lock
  rules, as in M1), `crates/selflowd/src/engine/mod.rs` (`holds_rule`),
  `crates/selflowd/src/ipc/handlers.rs` (`chosen_rule`, challenges from the rule)
- Modify: `locales/{en,pt-BR}/{default-config.toml,config.ftl,changes.ftl}`
- Test: `crates/selflow-core/tests/{config.rs,versions.rs}` rewritten; the fixture text in
  `crates/selflow-core/tests/support/mod.rs`, `crates/selflowd/tests/support/mod.rs` and
  `crates/selflowd/tests/ipc.rs` (`with_weekend`) moves to the new format

**Interfaces:**
- Consumes: `parse_duration`, `parse_clock` (M1).
- Produces (types derive `Debug, Clone, PartialEq, Eq`):
  - `pub struct Config { pub language: String, pub day_starts_at: Time, pub loosening_delay: SignedDuration, pub rule_sets: Vec<RuleSet>, pub exceptions: Vec<Exception> }`
    with `empty()`, `rule_set_for(day: Weekday) -> Option<&RuleSet>`,
    `rules_on(day: Weekday) -> &[Rule]` (empty when no set covers the day) and
    `exception_on(date: civil::Date) -> Option<&Exception>`.
  - `pub struct RuleSet { pub name: String, pub days: HashSet<Weekday>, pub rules: Vec<Rule> }`
    (the set's challenges are applied to its rules while parsing and not kept).
  - `pub struct Rule { pub id: String, pub label: String, pub start: Time, pub start_jitter: SignedDuration, pub end: RuleEnd, pub notices: Option<Vec<SignedDuration>>, pub message: Vec<String>, pub action: Action, pub challenges: Challenges }`
    with `enforces(&self) -> bool` (a lock or a shutdown) and
    `notice_offsets(&self) -> Vec<SignedDuration>`: the written list, sorted longest first, or,
    when the file leaves `notices` out (`None`), the default for the rule's action: 15, 5 and 1
    minute for a lock or a shutdown, `0m` otherwise.
  - `#[derive(Copy)] pub enum RuleEnd { At(Time), After(SignedDuration), DayEnd }`
  - `#[derive(Copy)] pub enum Action { Nothing, Lock { shutdown_after: Option<SignedDuration> }, ShutDown { repeat: bool } }`
  - `pub struct Exception { pub date: civil::Date, pub rules: Vec<String> }` with
    `covers(&self, rule_id: &str) -> bool` (an empty list covers every rule).
  - `Challenges`: unchanged.
  - `timing::parse_offset(text: &str) -> Result<SignedDuration, TimingError>`: like
    `parse_duration`, but zero is allowed and a negative value (`"-5m"`, `"5m ago"`, both valid
    jiff syntax) gives the new `TimingError::Negative(String)`.
  - In `versions`:
    `pub enum Scope { Setting, Rule { day: Weekday, rule: String }, Exception(civil::Date) }`;
    `pub struct UnitChange { pub scope: Scope, pub field: &'static str, pub id: &'static str, pub args: Vec<(&'static str, String)>, pub kind: ChangeKind }`
    with `unit(&self) -> String`; and `pub fn diff(old: &Config, new: &Config) -> Vec<UnitChange>`,
    one entry per message, per weekday, never merged. `classify(current, proposed) ->
    Classification` keeps its M1 shape and behavior on top of `diff`: entries merged across days
    the M1 way (`rule` and `days` arguments first), and the save's kind the loosest of them.
    `effective_at`, `in_force`, `pending` and `StoredVersion` stay as in M1 until Task 5.
- Unit keys: `Setting` gives the field (`language`, `day_starts_at`, `loosening_delay`); `Rule`
  gives `<day>/<rule>/<field>` with the day as `mon` to `sun`; `Exception` gives
  `exception/<date>`. Rule fields: `presence`, `timing` (start, jitter and end together), `action`,
  `notices`, `message`, `label`, `wait`, `reason`, `phrase`, `extensions`, `extension_length`.
  Fields that depend on each other share one unit, so they always share one clock (spec §8.9).
- Port in this task: `windows_for` builds M1's lock windows from the rules whose action is a lock
  and that no exception covers on that date, with `warnings` from `notice_offsets()`,
  `shutdown_after` from the action and `challenges` from the rule; `ends_at` by `RuleEnd` (`At`: M1's
  `next_after`; `After`: the start plus the length; `DayEnd`: `day_starts_at` on the next civil
  date). `holds_rule` and `chosen_rule` look at lock rules only.

**File format.** Keys are optional unless marked required. Unknown keys are syntax errors, as in
M1 (`deny_unknown_fields`); an M1 file therefore fails at `[[rule_set.lock]]`.

| Key | Value | Default |
|---|---|---|
| `language`, `day_starts_at`, `loosening_delay`, `[anchors]` | as in M1 | as in M1 |
| `[[exception]]` | `date` (required, `YYYY-MM-DD`, the logical date), `rules` (rule ids) | every rule |
| `[[rule_set]]` | `name`, `days` (both required) | |
| `[rule_set.challenges]` | as in M1; now the defaults of the set's rules | M1's |
| `[[rule_set.rule]]` | `id` (required), `label`, `start` (required: anchor or time), `start_jitter`, `end` (anchor or time) or `length` (duration), `notices` (offsets before the start, `"0m"` allowed), `message` (one text or a list), `lock` (bool), `shutdown_after` (with a lock), `shutdown` (`"once"` or `"repeat"`, without a lock) | label = id; jitter 0; ends at the next day boundary; notices `["15m", "5m", "1m"]` when the rule locks or shuts down, `["0m"]` otherwise; Selflow's texts; no lock; no shutdown |
| `[rule_set.rule.challenges]` | any of the challenge keys | the set's values |

`lock = true` makes the action a lock with `shutdown_after` (if any); `shutdown` without a lock
makes it a shutdown when the rule starts; otherwise it is nothing. Read `message` as
`Spanned<toml::Value>` and accept a string or an array of strings (serde's untagged enums lose
toml's spans). An omitted `notices` stays `None` in `Rule` and takes its default from the action
when it is used, so it never moves on a clock of its own while a change of action waits: spec §8.9
has changes that depend on each other take effect together.

**Errors**, each at its value: every M1 id except `config-duplicate-lock`, which becomes
`config-duplicate-rule` {id, set}; `config-empty-window` and `config-jitter-too-long` now speak of
a rule; the jitter check also covers rules that end at the day boundary (the start plus the jitter
must stay before it); `config-locks-overlap` compares only rules with a lock. New ids:

| Id | When | Args |
|---|---|---|
| `config-negative-offset` | a notice offset below zero | value |
| `config-end-and-length` | both `end` and `length` | id |
| `config-lock-needs-end` | `lock = true` with neither `end` nor `length` | id |
| `config-shutdown-after-needs-lock` | `shutdown_after` without `lock = true` | id |
| `config-shutdown-with-lock` | `shutdown` together with `lock = true` | id |
| `config-bad-shutdown` | `shutdown` other than `"once"` or `"repeat"` | value |
| `config-bad-message` | `message` neither a string nor a list of strings | id |
| `config-empty-message` | an empty text in `message` | id |
| `config-bad-date` | an exception date that isn't `YYYY-MM-DD` | value |
| `config-duplicate-exception` | the same date twice | date |
| `config-exception-unknown-rule` | a rule id the set for that date's weekday doesn't have | date, rule |

**How changes are judged** (`diff`). Rules match by id within the weekday. "Enforcing" means
`Rule::enforces` on either side. M1's ids and arguments stay where this table doesn't rename them.

| Unit field | Change | Message id | Kind |
|---|---|---|---|
| presence | added / removed | `change-rule-added` / `change-rule-removed` | Tighten / Loosen when enforcing, else Neutral |
| timing | M1 ruling R17's start bounds | `change-rule-starts-earlier` / `-later` | Tighten / Loosen when enforcing, else Neutral |
| timing | end position from the day start (`At`: M1's; `After`: start plus length; `DayEnd`: 24 h); old and new shown as clock times | `change-rule-ends-later` / `-earlier` | Tighten / Loosen when enforcing, else Neutral |
| action | nothing to lock / lock to nothing | `change-lock-added` / `change-lock-removed` | Tighten / Loosen |
| action | `shutdown_after` added, removed, sooner, later | M1's `change-shutdown-*` | M1's |
| action | nothing to shutdown / shutdown to nothing | `change-shutdown-at-start-added` / `-removed` | Tighten / Loosen |
| action | once to repeat / repeat to once | `change-shutdown-repeats` / `change-shutdown-once` | Tighten / Loosen |
| action | a lock to a shutdown, or back | the removal and the addition rows above, both in this unit | Loosen and Tighten |
| notices, message, label | any difference in what the file says (an omitted `notices` stays omitted whatever the action does) | `change-notices` (was `change-warnings`), `change-message`, `change-label` | Neutral |
| wait, reason, phrase, extensions, extension_length | M1's comparisons, now per rule | M1's ids, worded for a rule | M1's |
| settings | as in M1 | as in M1 | as in M1 |
| exception | added / removed | `change-exception-added` / `-removed` {date, rules} | Loosen / Tighten |
| exception | covers more rules / fewer / others | `change-exception-wider` / `-narrower` / `-changed` {date, rules} | Loosen / Tighten / Loosen |

The `rules` argument is the ids joined with `", "`, or `all` when the list is empty, which the
copy selects on (`{ $rules -> [all] ... *[other] ... }`).

Adding, removing or moving a rule that only sends notices is Neutral: spec §8.9 makes that so for
a rule whose only steps are notices or emails, because it enforces nothing (approved by the owner on
2026-11-20).

**Default files.** Same values in both languages, comments and the label in the file's language,
one comment line per key, as in M1: the M1 settings and anchors; rule set `every-night` (all
days) with M1's challenges; rule `bedtime` (label "Bedtime" / "Hora de dormir") from `bedtime` to
`wake`, `start_jitter = "15m"`, `notices = ["15m", "5m", "1m"]`, `lock = true`,
`shutdown_after = "30m"`. Commented-out examples follow, each with a one-line explanation: a
`message` list on the bedtime rule, a `[rule_set.rule.challenges]` override, a reminder rule with
only a `start` and a `message`, `shutdown = "once"` as the lock-free alternative, and an
`[[exception]]` with a note that adding one waits like any loosening, so it belongs at least a day
ahead.

**Fixture.** `VALID` in the core and daemon test support keeps M1's lines (so `wait` stays on line
15 and `start` on line 22); the lock becomes:

```toml
[[rule_set.rule]]
id = "bedtime"
start = "bedtime"
start_jitter = "15m"
end = "wake"
notices = ["5m", "15m", "1m"]
lock = true
shutdown_after = "30m"
```

- [ ] **Step 1: Write the failing tests** in `crates/selflow-core/tests/config.rs`:
  `parses_a_rule_with_a_lock_and_a_shutdown` (`VALID`: start 23:00, `RuleEnd::At(07:00)`, notices
  15, 5, 1 min, `Action::Lock { shutdown_after: Some(30m) }`, challenges inherited);
  `a_rule_overrides_its_sets_challenges`; `notices_default_by_action` (no `notices` key:
  `notices` is `None`, and `notice_offsets()` gives 15/5/1 min for a lock or a shutdown and `[0m]`
  for a rule with neither);
  `a_rule_can_shut_down_without_a_lock` (`shutdown = "once"` gives `ShutDown { repeat: false }`,
  `"repeat"` gives `true`, and no `end` gives `RuleEnd::DayEnd`); `a_message_is_one_text_or_a_list`;
  `an_exception_names_a_date_and_maybe_rules`; `reports_each_error_with_its_line`, a table of
  `(replacement applied to VALID, expected id, line)` with one row per new id and per renamed id;
  `the_default_files_parse_in_both_languages`. In `crates/selflow-core/tests/versions.rs`, keep
  M1's cases on the new fixture and add `diff_judges_every_field`, a table of
  `(replacement, field, expected id, expected kind)` with one row per line of the table above,
  checked on the entry for `thu`.
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflow-core`. Expected: FAIL.
- [ ] **Step 3: Implement** the format, the validation, `diff` and `classify`, and port the three
  consumers; move every fixture to the new format; write both default files and the new copy.
- [ ] **Step 4: Run** `proto exec rust -- make check`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: describe rules with any mix of steps`.

### Task 4: Each change on its own clock (`selflow-core::versions`)

**Files:**
- Modify: `crates/selflow-core/src/versions/{mod.rs,types.rs}`; Create:
  `crates/selflow-core/src/versions/clock.rs` (the clocks) and `assemble.rs` (the configuration in
  force)
- Modify: `crates/selflow-core/tests/support/mod.rs` (add `history_of`)
- Test: `crates/selflow-core/tests/clocks.rs`

**Interfaces:**
- Consumes: `Config`, `diff`, `UnitChange`, `Scope` (Task 3).
- Produces:
  - `pub struct SavedConfig { pub id: i64, pub saved_at: Timestamp, pub config: Config }`
  - `pub struct StoredChange { pub version_id: i64, pub unit: String, pub kind: ChangeKind, pub effective_at: Timestamp }`
  - `#[derive(Default)] pub struct History { pub saved: Vec<SavedConfig>, pub changes: Vec<StoredChange> }`
    (`saved` in ascending id order)
  - `pub struct TimedChange { pub change: UnitChange, pub effective_at: Timestamp }`
  - `pub fn plan_save(history: &History, proposed: &Config, saved_at: Timestamp) -> Vec<TimedChange>`
  - `pub fn config_at(history: &History, at: Timestamp) -> Config`
  - `pub fn waiting(history: &History, at: Timestamp) -> Vec<TimedChange>`
  - `pub fn rule_since(history: &History, at: Timestamp, day: Weekday, rule_id: &str) -> Option<Timestamp>`
  - `pub struct ShownChange { pub id: &'static str, pub args: Vec<(&'static str, String)>, pub effective_at: Timestamp }`
    and `pub fn shown(changes: &[TimedChange]) -> Vec<ShownChange>`: rule entries equal in rule,
    id, arguments and time merge across days the M1 way; the result is sorted by time, then by
    first appearance.
  - New ids in `changes.ftl`: `change-canceled` (a rule unit: "A change that was waiting for
    `{ $rule }` on { $days } won't happen."), `change-setting-canceled` {setting},
    `change-exception-canceled` {date}.
  - Test support: `pub fn history_of(saves: &[(ZonedDateTime, &str)]) -> History`, which parses
    each text, calls `plan_save`, and stores the version with one `StoredChange` per unit, as the
    daemon will (ids from 1).

**Rules the tests pin:**
- `plan_save` with no saved version: every entry of `diff(&Config::empty(), proposed)` takes effect
  at `saved_at` (nothing is enforced yet, so nothing can be loosened; M1 ruling R18).
- Otherwise, with `latest` the last saved config and `now` = `config_at(history, saved_at)`: the
  units this save changes are the units of `diff(latest, proposed)`. Each one's entries are the
  entries of `diff(now, proposed)` for that unit, judged against what is in force; when there are
  none, the save puts back the value in force, and the unit gets one Neutral `*-canceled` entry.
  A unit's kind is the loosest of its entries. Tighten and Neutral take effect at `saved_at`.
  Loosen takes the earliest of `saved_at + now.loosening_delay` (saturating) and the time of every
  waiting change of the same unit (effective after `saved_at`) whose version's value isn't
  stricter than the proposal for that unit (`diff(version, proposed)` has no Loosen entry for it).
  Units this save doesn't change keep their waiting changes and times.
- `config_at`: with nothing saved, `Config::empty()`. A unit's source is the highest version id
  among its stored changes in effect at `at`, else the first saved version. Settings come from
  their sources. Exceptions: every date any version names, from its source. Each weekday's
  candidate ids are every id any version has on that day; a rule is in force when its presence
  source has it, and each of its fields comes from that field's source, or from the presence
  source when that version lacks the rule on that day. Each weekday with rules becomes a rule set
  for that one day, named like the latest version's set for that day (else the presence source's),
  with the latest version's rule order first and other ids after, sorted. Set names carry no
  clock; from Task 6 on, nothing about timing depends on them (they only name things on screen
  and in test-lock slugs). The result is never validated: a lock that ends at the day boundary can
  only come from here, and every consumer handles it.
- `waiting`: stored changes effective after `at`, minus those a later version's change of the same
  unit overrides by then; for each one kept, the entries of `diff(config_at(history, at), version)`
  for its unit, with its time. Units with no entry left are dropped.
- `rule_since`: the latest time at or before `at` when the rule came into force on that day,
  which is the time of a stored change of its `presence` or `action` unit after which the rule has
  a lock or a shutdown, where the configuration in force an instant before had no rule by that id
  on that day, or one with another kind of action. `None` when there is none. Neutral saves and
  changes within one kind of action (a shorter `shutdown_after`, say) leave it alone, so repeated
  saves during a lock no longer move its shutdown (the M1 final review's first parked item).

- [ ] **Step 1: Write the failing tests** in `crates/selflow-core/tests/clocks.rs`, using
  `history_of`, `at` and `no_jitter`, with
  `let rule = |c: &Config| c.rules_on(Weekday::Thursday)[0].clone();` (2026-11-19 is a Thursday):

```rust
#[test]
fn a_tightening_and_a_loosening_in_one_save_keep_their_own_clocks() {
    let base = no_jitter();
    let both = base.replace(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#).replace("extensions = 1", "extensions = 3");
    let history = history_of(&[(at(18, 10, 0), &base), (at(18, 12, 0), &both)]);
    let now = config_at(&history, at(18, 12, 0).timestamp());
    assert_eq!(rule(&now).start, time(22, 0, 0, 0));
    assert_eq!(rule(&now).challenges.extensions, 1);
    assert_eq!(rule(&config_at(&history, at(19, 12, 0).timestamp())).challenges.extensions, 3);
}

#[test]
fn a_later_save_does_not_restart_a_waiting_change() {
    let base = no_jitter();
    let more = base.replace("extensions = 1", "extensions = 3");
    let renamed = more.replace("id = \"bedtime\"\n", "id = \"bedtime\"\nlabel = \"Sleep\"\n");
    let history = history_of(&[(at(18, 10, 0), &base), (at(18, 12, 0), &more), (at(18, 20, 0), &renamed)]);
    assert_eq!(rule(&config_at(&history, at(18, 20, 0).timestamp())).label, "Sleep");
    assert_eq!(rule(&config_at(&history, at(19, 12, 0).timestamp())).challenges.extensions, 3);
}
```

  Also, each over a first save of `no_jitter()` at 18 08:00 unless it says otherwise:
  `putting_back_the_value_in_force_cancels_the_waiting_change` (bedtime 01:00 saved at 18 10:00,
  23:00 saved at 18 12:00: `plan_save` for the second returns only `change-canceled` entries at
  18 12:00, and `config_at` at 19 11:00 still starts at 23:00);
  `a_value_between_the_one_in_force_and_a_waiting_one_keeps_the_waiting_clock` (01:00 saved at
  18 10:00, then 00:00 at 18 20:00: every entry of the second takes effect at 19 10:00);
  `the_first_save_takes_effect_at_once` (an empty history and a file with
  `loosening_delay = "1h"` and `day_starts_at = "04:00"`: every entry at `saved_at`);
  `a_lock_swapped_for_a_shutdown_waits_as_one_unit` (`lock = true` and `shutdown_after` replaced
  by `shutdown = "repeat"`: the `action` entries, removal and addition, all take effect at
  `saved_at + 24h`); `rule_since_is_when_the_rule_last_came_into_force` (here the first save at
  18 10:00 has no rules, then `VALID` at 19 23:40: `Some(19 23:40)` for Thursday, and neither a
  later label change nor a later `shutdown_after = "5m"` moves it);
  `waiting_lists_what_is_still_to_come` (`extensions = 3` saved at 18 12:00: at 18 13:00, one
  `change-more-extensions` per day at 19 12:00, and nothing at 19 12:00 itself);
  `shown_merges_the_days_of_one_change` (the same save: one entry whose first arguments are
  `("rule", "bedtime")` and `("days", "mon, tue, wed, thu, sun")`);
  `defaulted_notices_follow_the_action_in_force` (here the first save is `no_jitter()` without its
  `notices` line; a save at 18 12:00 drops `lock = true` and `shutdown_after`: at 18 12:00 the
  rule still locks and `notice_offsets()` gives 15/5/1 min, and at 19 12:00 it does nothing and
  gives `[0m]`).
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflow-core --test clocks`. Expected: FAIL.
- [ ] **Step 3: Implement** the rules above.
- [ ] **Step 4: Run** `proto exec rust -- make check`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: give each configuration change its own clock`.

### Task 5: The daemon on per-change clocks (store, save, status, report)

**Files:**
- Modify: `crates/selflowd/src/store/{mod.rs,migrations.rs,types.rs}`,
  `crates/selflowd/src/engine/mod.rs`, `crates/selflowd/src/ipc/{handlers.rs,types.rs}`,
  `crates/selflowd/src/app.rs`
- Modify: `crates/selflow-core/src/versions/{mod.rs,types.rs}` (M1's whole-save API removed)
- Modify: `crates/selflow-protocol/src/ipc/types.rs`, `crates/selflow/src/render/mod.rs`,
  `crates/selflow/src/cli/commands/config_cmd.rs`
- Modify: `locales/{en,pt-BR}/{cli.ftl,report.ftl,config.ftl}`
- Test: `crates/selflowd/tests/{store.rs,ipc.rs,engine.rs,support/mod.rs}`,
  `crates/selflow/tests/{render.rs,support/mod.rs}`, `crates/selflow-protocol/tests/ipc.rs`

**Interfaces:**
- Consumes: `History`, `SavedConfig`, `StoredChange`, `plan_save`, `config_at`, `waiting`,
  `rule_since`, `shown` (Task 4).
- Store migration 2 (append to `MIGRATIONS`, spec §8.3: the M1 versions don't parse any more, so
  the store starts over and keeps them aside; approved by the owner on 2026-11-20):

```sql
ALTER TABLE config_versions RENAME TO config_versions_m1;
CREATE TABLE config_versions (id INTEGER PRIMARY KEY, saved_at TEXT NOT NULL, text TEXT NOT NULL) STRICT;
CREATE TABLE config_changes (id INTEGER PRIMARY KEY, version_id INTEGER NOT NULL REFERENCES config_versions (id),
  unit TEXT NOT NULL, kind TEXT NOT NULL, effective_at TEXT NOT NULL) STRICT;
DELETE FROM started_windows;
```

- These two tables are the shape the server can keep in M3 (spec §20: how a saved
  configuration holds changes that take effect at different times).
- Store API: `save_version(text, saved_at, changes: &[StoredChange]) -> Result<i64, StoreError>`
  in one transaction, ignoring each change's `version_id` and using the new id;
  `history_rows() -> Result<(Vec<StoredVersion>, Vec<StoredChange>), StoreError>` (ascending);
  `latest_text()`; `m1_text() -> Result<Option<String>, StoreError>` (the newest M1 text). Core
  `StoredVersion` becomes `{ id, saved_at, text }`; `classify`, `Classification`, `Change`,
  `effective_at`, `in_force` and `pending` go away. Timestamps keep M1 ruling R28's format.
- Engine: parses each version once (a `HashMap<i64, Config>` cache; a version that doesn't parse
  is still `EngineError::StoredConfig`) and takes its configuration from `config_at`. The shutdown
  floor of M1 ruling R36 becomes `rule_since(history, now, window.logical_date.weekday(),
  &window.rule_id)` plus `shutdown_after`. `engine::config_in_force(store, now)` uses the same
  path.
- `ipc::Shared` gains `env_language: Language` (from `Language::from_env()` in `app.rs`), used
  wherever M1 read the environment for the language: the M1 final review's fourth parked item,
  whose test passed under an English locale either way.
- Protocol: `Response::Saved { saved_at: Timestamp, changes: Vec<ChangeView> }`;
  `pub struct ChangeView { pub message: MessageView, pub effective_at: Timestamp }`;
  `ConfigStatusView { last_saved_at: Option<Timestamp>, pending: Vec<ChangeView> }`; `ReportView`
  gains `pending: Vec<ChangeView>`. `ChangeKindView`, `PendingView` and the `report-pending` entry
  go away.
- Handlers: `SaveConfig` parses, calls `plan_save`, stores the text with one `StoredChange` per
  unit, records `config_saved` with detail `{"now": <units now>, "waiting": <units later>}`, and
  answers `shown` as `ChangeView`s. `ConfigStatus` and `Report` list `shown(waiting(...))`.
  `GetConfig` answers the latest text; with none, the default file in `env_language`, and when the
  store holds an M1 text, a blank line, the comment `# ` + `config-before-update`, and every line
  of the M1 text prefixed with `# ` (a bare `#` for an empty line).
- CLI: `render_saved(localizer, saved_at, changes, tz)` prints `cli-saved-now` and the changes at
  `saved_at`, then, per later time, `cli-saved-later` {when} and its changes; with no change,
  `cli-saved-nothing`. `render_config_status` prints `cli-status-last-saved` {when} (or
  `cli-status-default`) and the waiting changes grouped the same way under `cli-status-pending`
  {when}, or `cli-status-no-pending`. `render_report` ends with `cli-report-waiting` and one
  `cli-report-waiting-line` {when, text} per waiting change. The report renders
  `report-config-saved` {now, waiting} and keeps M1's three `report-config-saved-*` ids for nights
  stored before the update.

- [ ] **Step 1: Write the failing tests.** Store: `an_m1_store_opens_with_its_settings_kept_aside`
  (a file database built by hand with M1's migration 1 schema, `PRAGMA user_version = 1`, one
  `config_versions` row, one event and one started window: after `Store::open`, `latest_text()` is
  `None`, `m1_text()` is the old text, the event is still listed and the night log has no started
  window) and `a_save_keeps_each_change_with_its_time`. IPC:
  `a_save_applies_its_tightening_now_and_dates_its_loosening` (bedtime 22:00 and `extensions = 3`
  saved at 18 12:00 over `VALID`: the `Saved` changes hold `change-rule-starts-earlier` at 18 12:00
  and `change-more-extensions` at 19 12:00), `the_config_status_lists_each_waiting_change`,
  `after_an_upgrade_the_first_edit_shows_the_old_settings` (an M1 text in the store: the answer
  parses, starts with the default file, and contains `# [[rule_set.lock]]`), and
  `with_nothing_saved_the_status_uses_the_environment_language` (with `env_language: PtBr`).
  Engine: `a_loosening_waits_while_a_tightening_from_the_same_save_applies` (the save at 18 12:00:
  the lock shows at 18 22:00 with 1 extension left, and at 19 22:00, after the delay, with 3),
  and M1's `a_rule_that_just_came_into_force_waits_its_full_shutdown_time` and
  `a_neutral_save_during_a_lock_does_not_postpone_its_shutdown` on the new store API. Render:
  `a_save_lists_what_applies_now_and_what_waits`. Protocol: the round trip covers `Saved` with two
  `ChangeView`s.
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflowd -p selflow -p selflow-protocol`.
  Expected: FAIL.
- [ ] **Step 3: Implement** the migration, the store API, the engine change, the handlers, the
  protocol, the rendering and the copy.
- [ ] **Step 4: Run** `proto exec rust -- make check`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: store each change with the time it takes effect`.

### Task 6: Rules beyond the lock (notices, budgets, exception days)

**Files:**
- Modify: `crates/selflow-core/src/plan/{mod.rs,types.rs}`, `crates/selflow-core/src/enforce/{mod.rs,types.rs}`,
  `crates/selflow-core/src/config/types.rs` (serde derives on `Action`)
- Modify: `crates/selflowd/src/engine/{mod.rs,types.rs}`, `crates/selflowd/src/store/mod.rs`,
  `crates/selflowd/src/ipc/handlers.rs`
- Modify: `locales/{en,pt-BR}/warnings.ftl`
- Test: `crates/selflow-core/tests/{enforce.rs,plan.rs,support/mod.rs}`,
  `crates/selflowd/tests/{engine.rs,ipc.rs,support/mod.rs}`

**Interfaces:**
- Consumes: `Rule`, `Action`, `RuleEnd`, `Exception` (Task 3); `config_at` through the engine
  (Task 5).
- Produces (renames: the code now covers every rule, and the spec calls warnings notices):
  - `LockWindow` becomes `#[derive(Serialize, Deserialize)] pub struct RuleWindow { pub set: String, pub rule_id: String, pub label: String, pub logical_date: civil::Date, pub starts_at: ZonedDateTime, pub ends_at: ZonedDateTime, pub notices: Vec<SignedDuration>, pub message: Vec<String>, pub action: Action, pub challenges: Challenges, #[serde(default)] pub test: bool }`
    with `key(&self) -> String` returning `rule_id`: one set covers a weekday, so the id names the
    rule for its date, and renaming a set mid-night no longer changes the key (an M1 deferred item).
  - `windows_for` and `windows_around` return one `RuleWindow` per rule that no exception covers on
    that date, with `notices` from `notice_offsets()`. The jitter is drawn from the seed, the rule
    id and the date, where M1 drew it from `<set>/<rule>`: a set's name carries no clock (Task 4),
    so a rename would otherwise move a start later at once. The minute drawn for each rule changes
    once with the update, inside the rule's window.
  - `NightLog` becomes `#[derive(Default)] pub struct DayLog { pub extensions: Vec<GrantedExtension>, pub started: Vec<RuleWindow>, pub exceptions: Vec<GrantedException> }`;
    `pub struct GrantedException { pub rule_id: String, pub logical_date: civil::Date, pub granted_at: Timestamp, pub reason: String }`
    (stored from Task 9).
  - `Warning` becomes `pub struct Notice { pub key: String, pub label: String, pub at: Timestamp, pub moment: Timestamp, pub variant: u8, pub kind: NoticeKind, pub message: Option<String> }`
    with `pub enum NoticeKind { Lock, ShutDown, Reminder }`; `Evaluation.warnings` becomes
    `notices`.
  - `GrantedExtension` keeps its fields; extensions match a window by `rule_id` and logical date.
- Test-lock windows take the rule id `<id>#test-<unix seconds>` (M1 marked the rule set), the
  action `Lock { shutdown_after: None }` (ruling R40), and come from lock rules only; the list of
  valid slugs in `ipc-test-lock-unknown-rule` is deduplicated, because a configuration in force
  can hold one set per weekday.

**Rules the tests pin** (M1's rules hold where these don't speak):
- Candidates: the computed windows and the started ones of the same rule id and date, merged with
  M1 ruling R26's stricter rule plus two points. `starts_at` is the started one when it is at or
  before now, else the earlier of the two, so a tightening still reaches a lock whose notices went
  out. The action is the started one, made stricter by the computed one when both are of the same
  kind (a lock's `shutdown_after`: the shorter, `Some` beating `None`; a shutdown: `repeat` when
  either repeats). `message` comes from the computed window. Windows of a rule with a granted
  exception for that date drop out, started ones included.
- Only windows whose action is a lock can be locked. `extensions_left` counts the extensions of the
  same rule id and logical date: one budget per rule per day (spec §8.5), where M1 ruling R25
  counted the rule set's.
- Notices: a lock window lists them as in M1. Any other window lists every notice before its start
  while `now < ends_at`, past ones included, and the engine's grace decides. The kind follows the
  action (`Reminder` for nothing); before the end of a running extension it is `Lock`. `variant` is
  drawn as in M1. `message` is `Some(message[i])` when the rule has texts, `i` drawn from the seed,
  the key and the logical date, so it changes from day to day but not within a day.
- Engine: the title is `notice-title` {label, time}, except on `ShutDown` notices, whose title is
  `notice-shutdown-title` {label, time} and says the computer shuts down, so a custom body never
  hides that. The body is the custom text when there is one, else `warning-body-<variant>` for
  `Lock`, `notice-shutdown-body-<variant>` for `ShutDown`, and `notice-reminder-body-<variant>`
  for `Reminder` {minutes as a number, time}; the reminder bodies select on `[0]` for a notice at
  the start. An enforcing window is recorded with
  `record_started` when its first notice goes out, not only when its lock starts, so a loosening
  that comes into force between the notice and the lock can't move or cancel it (spec §8.9; M1
  ruling R27, deferred until now). `Status.next_lock_at` looks at lock windows only.
- `notice-title` replaces `warning-title` in `warnings.ftl`, next to the new
  `notice-shutdown-title`; M1's `warning-body-*` stay.

- [ ] **Step 1: Write the failing tests.** Core support gains `REMINDER` (a rule set `everyday`,
  all days, `[[rule_set.rule]]` `id = "stretch"`, `start = "15:00"`,
  `message = ["Stand up for a minute.", "Time to stretch."]`) and `TWO_LOCKS` (`no_jitter()` with a
  second lock rule `nap` from 14:00 to 14:30). In `enforce.rs`:

```rust
#[test]
fn a_rule_without_a_lock_notifies_at_its_start_and_never_locks() {
    let c = parse(REMINDER).unwrap();
    let before = evaluate(&at(19, 14, 0), &c, Seed(7), &DayLog::default());
    assert_eq!(before.notices.iter().map(|n| n.at).collect::<Vec<_>>(), vec![at(19, 15, 0).timestamp()]);
    assert_eq!(before.notices[0].kind, NoticeKind::Reminder);
    assert_eq!(before.next_change_at, Some(at(19, 15, 0).timestamp()));
    assert!(matches!(evaluate(&at(19, 15, 0), &c, Seed(7), &DayLog::default()).enforcement, Enforcement::Free));
}
```

  Also: `a_custom_message_picks_one_text_per_day` (the chosen text is one of the two and the same
  at 14:00 and 14:30); `each_rule_has_its_own_extension_budget` (`TWO_LOCKS`, one extension spent
  in `nap`: bedtime at 23:00 still has 1 left); `an_exception_day_skips_its_rules` (an
  `[[exception]]` for 2026-11-19 with `rules = ["bedtime"]`: free at 19 23:30, locked at
  18 23:30); `a_granted_exception_drops_a_started_window`;
  `a_loosening_after_the_first_notice_does_not_move_the_coming_lock` (started = `windows_for` of
  the 19th; with bedtime at 01:00 in the configuration, still locked at 19 23:00);
  `a_tightening_after_the_first_notice_brings_the_lock_earlier` (the same started window; with
  bedtime at 22:50, locked at 19 22:50). `plan.rs`: `windows_end_at_their_length_or_the_day_boundary`
  and `renaming_a_rule_set_keeps_the_start` (`VALID`, with its 15-minute jitter, and the same text
  with the set named `nights`: the same `starts_at` on the 19th). Engine:
  `a_reminder_rule_shows_its_own_text_at_its_start` (`Call::Warn` with one of the two texts at
  15:00), `the_first_notice_records_the_window` (after a tick at 22:45, the night log of the 19th
  holds the bedtime window), and `a_shutdown_notice_with_its_own_text_still_says_it_shuts_down` (a
  rule with `shutdown = "once"`, `notices = ["10m"]` and `message = "Time to stop."`: the warning's
  body is that text and its title is the rendered `notice-shutdown-title`; the support's `Calls`
  gains `last_title()`).
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflow-core -p selflowd`. Expected: FAIL.
- [ ] **Step 3: Implement**, renaming through every consumer and test.
- [ ] **Step 4: Run** `proto exec rust -- make check`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: send notices for every rule`.

### Task 7: Shutdown without a lock

**Files:**
- Modify: `crates/selflow-core/src/enforce/{mod.rs,types.rs}`,
  `crates/selflowd/src/engine/{mod.rs,types.rs}`, `crates/selflowd/src/store/{mod.rs,types.rs}`,
  `crates/selflowd/src/ipc/handlers.rs` (report entry)
- Modify: `locales/{en,pt-BR}/{warnings.ftl,report.ftl}`
- Test: `crates/selflow-core/tests/enforce.rs`, `crates/selflowd/tests/{engine.rs,support/mod.rs}`

**Interfaces:**
- Consumes: `RuleWindow`, `DayLog`, `Notice` (Task 6); `SHUTDOWN_NOTICE` (M1, 60 s).
- Produces:
  - `DayLog.shutdowns: Vec<ShutdownDone>` with
    `pub struct ShutdownDone { pub rule_id: String, pub logical_date: civil::Date, pub at: Timestamp }`.
  - `Evaluation.shutdown: Option<RuleWindow>`: the earliest window whose action is a shutdown, with
    `starts_at <= now < ends_at`, that repeats or has no `ShutdownDone` for its rule id and date.
  - `EventKind::RuleShutdown`, stored as `rule_shutdown`, with detail
    `{"label": ..., "rule": <rule id>, "repeat": <bool>}`; its key is `<rule>/<date>` when the
    shutdown happens once and `<rule>/<date>/<at>` when it repeats. `Store::night_log` fills
    `shutdowns` from these events. Report id `report-rule-shutdown` {label}.
  - New copy: `notice-shutdown-now` {time}, the body of the notice Selflow adds when nothing
    announced the shutdown yet; its title is Task 6's `notice-shutdown-title`.

**Rules the tests pin** (spec §8.3):
- When `Evaluation.shutdown` names a window and no lock is on screen, the engine looks for the
  instant it sent its first notice of that window during this run: the tick at which the notice
  went out, which can come up to `WARNING_GRACE` after the notice's own time, never that own time.
  With none, it sends `notice-shutdown-now` at once, whatever the rule's message says, and counts
  from then. The shutdown comes at the later of the window's start and that instant plus
  `SHUTDOWN_NOTICE`, never earlier; until then the engine wakes for it.
- While a lock is on screen, desktop notices don't show over the session lock. A due shutdown
  rule then joins the lock view instead: the view's `shutdown_at` becomes the earlier of the lock's
  own shutdown time and the rule's start, under the "first appeared" rule of the last item here,
  so the minute counts from when the time first appears on the lock screen; that instant also
  counts as the window's first notice if the lock ends before the shutdown. When the moment comes
  and the rule's time is the one on screen, the engine records the rule's `rule_shutdown` event as
  below and shuts down once.
- At that moment it records the `rule_shutdown` event, calls `shut_down` once per window per
  run, and remembers it. `once` is never due again that day, after a restart or a resume
  included; `repeat` is due again on every run until `ends_at`. A shutdown without a lock has no
  extension.
- A lock's shutdown (the M1 final review's fifth parked item): the engine remembers, per shown
  lock, the shutdown time on screen and when that time first appeared. When the time it computes
  is earlier than the one on screen, or appears where there was none, "first appeared" becomes
  now. The shutdown comes at the later of the computed time and "first appeared" plus
  `SHUTDOWN_NOTICE`, which keeps M1's minute after the screen first showed a time. A shutdown
  that lands at or after the lock's end still doesn't happen (M1 ruling R39).

- [ ] **Step 1: Write the failing tests.** Daemon support gains `LIGHTS_OUT` (rule set `always`, all
  days; rule `lights-out` with `start = "02:00"`, `end = "04:00"`, `notices = []`,
  `shutdown = "once"`) and `start_engine_with(text, now)`, which saves `text` as the first version
  through `save_text` (at 18 10:00) and returns the engine, the calls, the clock and the store. Engine tests, with the 20th at 02:00
  (logical date the 19th):
  `a_shutdown_without_a_lock_gives_a_minute_of_notice` (tick at 02:00: one `Warn`, no `ShutDown`;
  at 02:00:59: still none; at 02:01: `ShutDown`);
  `once_stays_up_after_a_restart` (after that, a new engine on the same store at 02:30: no `Warn`,
  no `ShutDown`); `repeat_shuts_down_again_after_a_restart_until_the_rule_ends` (with
  `shutdown = "repeat"`: a new engine at 02:30 warns and shuts down at 02:31; a new engine at
  04:00 does neither); `configured_notices_count_as_the_minute` (with `notices = ["10m"]`: a warn
  at 01:50, and `ShutDown` at the 02:00 tick);
  `a_resume_after_the_start_still_gets_its_one_shutdown` (one engine ticks at 01:00 and next at
  02:20, as after a suspend: a warn at 02:20 and `ShutDown` at 02:21);
  `the_minute_counts_from_when_the_notice_went_out` (with `notices = ["1m"]`, a daemon whose first
  tick is at 02:00:30: the 01:59 notice goes out then, and `ShutDown` doesn't come before
  02:01:30); `a_shutdown_rule_during_a_lock_shows_on_the_lock_screen` (`no_jitter()` without
  `shutdown_after`, plus the `lights-out` rule in the same set: with the bedtime lock shown since
  19 23:00, the tick at 20 02:00 sets the view's `shutdown_at` to 02:01, and `ShutDown` comes at
  02:01, not before);
  `a_save_during_a_lock_that_brings_the_shutdown_forward_still_gives_a_minute` (a lock shown at
  23:00 with its shutdown at 23:30; at 23:10 a stricter save sets `shutdown_after = "5m"`: the
  view's `shutdown_at` reads 23:11 and `ShutDown` doesn't come before 23:11).
  Core: `a_shutdown_rule_is_due_from_its_start_until_its_end` and
  `once_is_not_due_after_it_happened`.
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflow-core --test enforce -p selflowd --test engine`.
  Expected: FAIL.
- [ ] **Step 3: Implement** the rules above.
- [ ] **Step 4: Run** `proto exec rust -- make check`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: shut down without a lock`.

### Task 8: Unlock from a terminal (`selflow unlock`) and releasing a stuck screen

**Files:**
- Create: `crates/selflow/src/cli/commands/unlock_cmd.rs`, `crates/selflow/src/prompt/mod.rs`
- Modify: `crates/selflow/src/{lib.rs,cli/mod.rs,cli/types.rs,cli/commands/mod.rs}`,
  `crates/selflow-protocol/src/ipc/types.rs`,
  `crates/selflowd/src/ipc/{handlers.rs,types.rs}`, `crates/selflowd/src/enforcer/{mod.rs,lock_supervisor.rs}`,
  `crates/selflowd/src/app.rs`
- Modify: `locales/{en,pt-BR}/{cli.ftl,ipc.ftl,report.ftl}`
- Test: `crates/selflow/tests/{unlock.rs,support/mod.rs}`, `crates/selflowd/tests/{ipc.rs,lock_supervisor.rs,support/mod.rs}`,
  `crates/selflow-protocol/tests/ipc.rs`

**Interfaces:**
- Consumes: `LockView` and `RequestExtension` (M1), `Client` (Task 2), `LockSupervisor` (M1).
- Protocol: `Request::RequestExtension` gains `#[serde(default)] source: ExtensionSource` with
  `#[derive(Default)] #[serde(rename_all = "snake_case")] pub enum ExtensionSource { #[default] Screen, Terminal }`;
  new `Request::ReleaseScreen` and `Response::Released`.
- Daemon: a granted extension's event detail gains `"source": "terminal"` when it came from the
  terminal, and `report_entry` then uses `report-extension-terminal` with the same args.
  `ipc::Shared` gains `screen: Arc<dyn ScreenControl>`, with
  `pub trait ScreenControl: Send + Sync { fn release(&self); }`, implemented by `LockSupervisor`
  (made `Clone` over its existing `Arc`) and by a recording fake in the tests. `ReleaseScreen`
  answers `Error(ipc-release-locked)` while a view is shown, else calls `release()` and answers
  `Released`.
- `LockSupervisor` (spec §8.13): `release()` stops any client still running and starts one
  release client, tracked like any client and not restarted when it exits while no view is shown.
  It subscribes, gets `LockEnded` at once (M1 ruling R31), takes over the session lock
  (`allow_session_lock_restore`) and lets it go. When `hide()`'s grace ends with the client still
  running, the supervisor kills it and then starts one release client. Every release client gets
  `HIDE_GRACE` to exit and is killed after it, with no further release. The two items the M1
  final review parked here: `quick_exits` goes back to 0 whenever a client ran at least
  `STEADY_RUN`, even when no view is shown, and a test pins that `show()` respects the backoff.
- `selflow::prompt`: `pub trait Prompt { fn ask(&mut self, question: &str) -> Option<String>; fn say(&mut self, text: &str); fn wait_until(&mut self, at: Timestamp); }`,
  `TerminalPrompt` (stdin and stdout; `wait_until` sleeps), and
  `pub fn answer_challenges(challenge: &Challenge, localizer: &Localizer, tz: &TimeZone, prompt: &mut dyn Prompt, submit: &mut dyn FnMut(String, String) -> Result<Response, CliError>) -> Result<Response, CliError>`
  with `pub struct Challenge { pub label: String, pub available_at: Timestamp, pub reason_required: bool, pub phrase: Option<String>, pub extensions_left: u32 }`
  (built from a `LockView` here and from Task 9's view there). The loop: no extension left gives
  the refusal at once; otherwise wait until `available_at` (saying `cli-unlock-wait` {time}), ask
  the reason when required (`lock-reason-label`) and the phrase when there is one
  (`lock-phrase-label`), submit, and on `ReasonTooShort` or `PhraseMismatch` say the refusal's
  `lock-refused-*` text and ask again; on `TooEarly` wait again; `None` from `ask` (end of input)
  stops with the last answer. Any other answer is returned.
- `selflow unlock` (`UnlockArgs` has no flags): `unlock_cmd(args)` only connects to
  `socket_path()` and calls the seam
  `pub fn unlock_with(client: &mut Client, prompt: &mut dyn Prompt, tz: &TimeZone) -> Result<ExitCode, CliError>`,
  which says everything through `prompt.say`. It sends `Status` first, for the language and the
  lock. With no lock, it sends `ReleaseScreen` and says `cli-unlock-not-locked`. With a lock,
  `answer_challenges` with `source: Terminal`, then `cli-unlock-granted` {time} or the refusal
  text; exit 0 when granted, 1 otherwise. An unreachable daemon prints M1's
  `cli-daemon-unreachable`, which says how to start it, and exits 2 (spec §8.13).

- [ ] **Step 1: Write the failing tests.** CLI support gains `ScriptedPrompt` (answers from an
  array, `None` when they run out, and records what it said and every `wait_until`) and
  `FakeDaemon` (a thread with its own tokio runtime that listens through
  `selflow_platform::transport` on a temporary socket, answers each request with the next scripted
  `Response`, and records the requests; `FakeDaemon::client()` connects a `Client` to it).
  `crates/selflow/tests/unlock.rs` calls `answer_challenges` and `unlock_with`, never
  `unlock_cmd`: `waits_then_asks_the_reason_and_the_phrase` (it waited until `available_at` and
  submitted both answers), `asks_again_after_a_short_reason`, `stops_when_no_extension_is_left`
  (nothing submitted), and `unlock_with_nothing_locked_releases_the_screen` (the fake daemon
  answers `Status` with no lock: the next request is `ReleaseScreen`, and the prompt said the
  `cli-unlock-not-locked` text). IPC:
  `an_extension_from_a_terminal_is_marked_in_the_report`, `releasing_is_refused_during_a_lock`,
  and `releasing_with_nothing_locked_starts_one_client` (the fake recorded one `release`).
  Supervisor, with `sh -c 'echo started >> "$0"; exec sleep 30'` writing to a temporary file:
  `a_release_starts_one_client_that_is_not_restarted`,
  `a_client_killed_after_the_lock_ends_is_followed_by_one_release_client` (show, hide, and about
  11 s later exactly two starts), `quick_exits_reset_after_a_steady_client_is_stopped`, and
  `show_waits_out_the_backoff`. Protocol: an M1-shaped `request_extension` line without `source`
  reads as `Screen`.
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflow -p selflowd -p selflow-protocol`.
  Expected: FAIL.
- [ ] **Step 3: Implement** the protocol, the daemon side, the supervisor, the prompt module and
  the command.
- [ ] **Step 4: Run** `proto exec rust -- make check`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: unlock from a terminal`.

### Task 9: Exceptions on the same day (`selflow exception`)

An exception added to the file waits like any loosening, so one added at least a day ahead passes
without friction (Task 3 and Task 4 already give that). This task covers the same day: the rule's
challenges, as spec §8.9 asks. A same-day exception also needs an extension left for that rule,
and is refused while that rule's lock is on screen; otherwise it would cost less than one
extension while freeing the whole night (Claude's call, approved by the owner on 2026-11-20).

**Files:**
- Create: `crates/selflow/src/cli/commands/exception_cmd.rs`
- Modify: `crates/selflow-core/src/challenge/{mod.rs,types.rs}`, `crates/selflow-protocol/src/ipc/types.rs`,
  `crates/selflowd/src/store/{mod.rs,migrations.rs,types.rs}`, `crates/selflowd/src/ipc/{handlers.rs,types.rs}`,
  `crates/selflowd/src/engine/mod.rs`, `crates/selflow/src/cli/{mod.rs,types.rs,commands/mod.rs}`
- Modify: `locales/{en,pt-BR}/{cli.ftl,ipc.ftl,report.ftl}`
- Test: `crates/selflow-core/tests/challenge.rs`, `crates/selflowd/tests/ipc.rs`,
  `crates/selflow/tests/exception.rs`, `crates/selflow-protocol/tests/ipc.rs`

**Interfaces:**
- Consumes: `answer_challenges`, `Challenge`, `ScriptedPrompt` (Task 8); `GrantedException`,
  `DayLog.exceptions` (Task 6).
- Core: `challenge::request_exception(now: Timestamp, requested_at: Timestamp, rule_id: &str, logical_date: civil::Date, challenges: &Challenges, extensions_left: u32, answer: &ExtensionAnswer) -> Result<GrantedException, Refusal>`,
  with `request_extension`'s checks in its order: `TooEarly` before `requested_at + wait`,
  `NoBudget` with no extension left for the rule that day, then the reason and the phrase. A rule
  that neither locks nor shuts down skips every check, because skipping its notices is neutral.
- Store migration 3: `CREATE TABLE exceptions (rule_id TEXT NOT NULL, logical_date TEXT NOT NULL, requested_at TEXT NOT NULL, granted_at TEXT, reason TEXT, PRIMARY KEY (rule_id, logical_date)) STRICT;`
  The request's start lives in the store, so its wait carries across restarts: a `repeat`
  shutdown gives one minute per boot, less than the default 2-minute wait.
  `start_exception(rule_id, date, at) -> Result<Timestamp, StoreError>` inserts the row unless one
  exists and returns the stored, earliest start; `grant_exception(&GrantedException)` fills
  `granted_at` and `reason`; `night_log` fills `DayLog.exceptions` from the granted rows.
  `EventKind::ExceptionGranted` (`exception_granted`), detail `{label, reason}`, report id
  `report-exception` {label, reason}.
- Protocol: `Request::StartException { rule: String }` answered by
  `Response::ExceptionChallenge(ChallengeView)` with
  `pub struct ChallengeView { pub label: String, pub available_at: Timestamp, pub reason_required: bool, pub phrase: Option<String>, pub extensions_left: u32 }`;
  `Request::ConfirmException { rule: String, reason: String, phrase: String }` answered by
  `Response::ExceptionGranted { label: String }` or `Response::ExceptionRefused(RefusalView)`.
- Daemon: `StartException` refuses with an `Error` when the rule isn't among today's rules
  (`ipc-exception-unknown-rule` {rule, rules}), when that rule's lock is on screen
  (`ipc-exception-locked` {label}: an exception is not a way around a running lock), or when it is
  already skipped today (`ipc-exception-done` {label}); otherwise it calls `start_exception` and
  answers the challenge from the stored start. `ConfirmException` with no stored start answers
  `ipc-exception-not-started`. A granted exception is stored, recorded and wakes the engine, whose
  windows for that rule and date drop out (Task 6), a due shutdown's included.
- `selflow exception --rule <id>`: `exception_cmd(args)` only connects and calls the seam
  `pub fn exception_with(client: &mut Client, rule: &str, prompt: &mut dyn Prompt, tz: &TimeZone) -> Result<ExitCode, CliError>`:
  `StartException`, then `answer_challenges` sending `ConfirmException`, then
  `cli-exception-granted` {label}; exit codes as `selflow unlock`.

- [ ] **Step 1: Write the failing tests.** Core: `an_exception_waits_from_its_request`,
  `an_exception_needs_an_extension_left`, `a_rule_with_only_notices_is_skipped_without_challenges`.
  IPC: `an_exception_is_refused_while_its_lock_is_on_screen`,
  `an_exception_is_granted_after_the_wait_and_shows_in_the_report` (start at 20:00, a confirm at
  20:01 refused `TooEarly`, a confirm at 20:02 with a reason granted; the 19th's report lists
  `report-exception`; an engine on the same store, built as in M1's test-lock engine test with
  `shared_over`, doesn't lock at 23:00), `an_unknown_rule_is_refused_with_todays_rules`, and
  `the_wait_of_an_exception_carries_across_a_restart` (a `repeat` rule `lights-out` from 02:00;
  `StartException` at 20 02:00:30; a second `Shared` over the same store, as after a restart,
  grants a `ConfirmException` with a reason at 02:03, and an engine on that store then has no
  shutdown due). CLI: `exception_waits_asks_and_reports`, calling `exception_with` with
  `ScriptedPrompt` and `FakeDaemon`. Protocol: the round trip covers the four new messages.
- [ ] **Step 2: Run** `proto exec rust -- cargo test -p selflow-core -p selflowd -p selflow -p selflow-protocol`.
  Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `proto exec rust -- make check`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: skip a rule for the day`.

### Task 10: Install, owner check and measurement

**Files:**
- Modify: `README.md` (install and use; the agent writes it), `contrib/hyprland.lua` only if a
  step below proves it wrong

- [ ] **Step 1: README.** `make install` now installs `selflowd`, `selflow` and `selflow-ui`. After
  the update, `selflow config edit` opens the new default file with the M1 settings as comments at
  the end, so set them again there, outside lock hours. Then the new file in short: rules,
  `notices`, `message`, `lock` and `shutdown_after`, `shutdown = "once"` or `"repeat"`, challenges
  per rule, `[[exception]]`; then `selflow config status`, `selflow unlock` (also the way out of a
  stuck lock screen, which replaces the M1 note), `selflow exception --rule <id>`,
  `selflow test-lock`, and `selflow-ui --preview`. Run the humanizer skill on the prose.
- [ ] **Step 2: Hand the owner this checklist** and wait for the results; a failure goes back as a
  bug to the task that owns the code. Before it, the owner updates any idle-locker guard in their
  own configuration from `pgrep -f "selflow lock"` to `pgrep -f "selflow-ui --lock"` (the agent
  shows the line and doesn't edit dotfiles). In a Hyprland session, after `make install`, with
  `systemctl --user edit selflowd` adding `Environment=SELFLOW_POWER=log`, then
  `systemctl --user restart selflowd` and `journalctl --user -u selflowd -n 5` showing
  `power mode: log`:
  1. `selflow config edit`: the new default file opens, with the M1 settings as comments at the
     end. Set bedtime and wake as before, outside their hours, and save: everything applies now.
  2. Add a rule `stretch` 3 minutes ahead with only `start` and
     `message = "Stand up and stretch."`. At its time the notification shows that text, and
     nothing locks.
  3. Add a rule `off` 3 minutes ahead with `shutdown = "once"`, `end` 20 minutes ahead and
     `notices = []`. At its time a notice says the computer shuts down in a minute, and a minute
     later the journal shows `shutdown requested`. `systemctl --user restart selflowd`: nothing
     more. Edit it to `shutdown = "repeat"` (stricter, so it applies now) and restart the daemon
     again: a new notice, and a minute later `shutdown requested` again.
  4. `selflow test-lock --for 10m`: every monitor shows the lock from `selflow-ui`. Switch to a
     TTY (Ctrl+Alt+F3), log in, run `selflow unlock`: it waits out the 2-minute wait, asks for a
     reason, and says until when. Back in Hyprland the screen is free, and
     `selflow report --date <today>` lists the extension as one from a terminal.
  5. Once the first test lock has ended (`selflow test-lock` refuses while one is still open),
     `selflow test-lock --for 10m` again, then from the TTY `pkill -STOP -f "selflow-ui --lock"`
     and `selflow unlock`: after the grant, within about 6 s, the frozen client is replaced and the
     screen is free.
  6. `selflow config edit`: move bedtime 30 minutes earlier, still ahead of the current time (an
     earlier bedtime applies at once, so one already past locks the screen), and set
     `extensions = 2`, in one save.
     The answer lists the bedtime under "applies now" and the extensions under tomorrow at this
     time. `selflow config status` lists the extensions with that date; after one more save that
     only changes the label, the date is the same.
  7. `selflow exception --rule bedtime`: it waits 2 minutes, asks for a reason and says bedtime is
     skipped today; `selflow report --date <today>` lists it.
  8. `selflow-ui --preview` shows the lock screen in a window; Escape closes it.
  9. Reset, because steps 2, 3, 6 and 7 leave changes that would act for real later: stop the
     daemon, move `~/.local/state/selflow/selflow.db` aside, remove the `SELFLOW_POWER` override,
     start the daemon, and run `selflow config edit` with the real settings outside lock hours.
- [ ] **Step 3: Measure the idle daemon** for 10 minutes outside any rule, with the read-only
  commands of M1's Task 16 Step 6: RSS under 30720 KiB, and under 1 s of CPU in total.
- [ ] **Step 4: Commit** `feat: run rules with any mix of notices, locks and shutdowns` once the
  owner reports the checklist passing, with both measurements in the body, staging only the
  README and anything Step 2 fixed.
