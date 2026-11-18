# Selflow M1 "Night lock" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On one machine, Selflow warns before bedtime, locks every screen with a real Wayland
session lock, grants extensions behind a wait and a written reason, shuts the machine down when the
lock lasts too long, and makes configuration changes that loosen it wait 24 hours.

**Architecture:** A Cargo workspace laid out as in spec §19. `selflow-core` holds every decision as
pure functions of the clock, the configuration and a log. `selflowd` runs as a systemd user
service: it evaluates the core in a loop that sleeps until the next change (never longer than
60 s), keeps its state in a local SQLite file, and drives notifications, the lock client and
shutdown. `selflow` is the command line and the iced lock screen, and both talk to the daemon over
a Unix socket, one JSON object per line. There is no server in this milestone.

**Tech Stack:** Rust 2024 · tokio · serde, toml · jiff · fastrand · thiserror · tracing,
tracing-subscriber · fluent-bundle, unic-langid · rusqlite (bundled) · serde_json · clap ·
notify-rust · tempfile · iced 0.14 · iced_exwlshell 0.21 (`sessionlock` module).

**Spec:** `docs/superpowers/specs/2026-11-17-selflow-design.md`. This plan is milestone M1 of
`docs/superpowers/plans/2026-11-18-selflow-phase-1-roadmap.md`.

**Repository root:** `/home/talinux/Workspace/github.com/ItzTas/personal-projects/selflow`. Every
path in this plan is relative to it unless it starts with `/` or `~`.

**Status:** not started. Run it with subagents as described in "Executing this plan with
subagents", after reading "Agent safety".

## Global Constraints

- English for code, identifiers, logs and source copy. Every string a person sees goes through
  `selflow-i18n`, and `en` and `pt-BR` are both complete (Task 2 adds the test that enforces it).
  Copy is calm, without guilt or alarm wording (spec §4).
- Layout from spec §19. `main.rs` holds only `main`. `lib.rs` only declares modules. One module per
  subject, which becomes a folder with `mod.rs`, `types.rs` and `constants.rs` as it grows. Each
  subject module owns its error enum, built with `thiserror`. CLI commands live in
  `src/cli/commands/<name>_cmd.rs` with `<Name>Args` and `<name>_cmd()`.
- No comments in the code, as in the owner's mime-guardian and diario-alerr.
- `selflowd` never depends on `iced` or `iced_exwlshell`; only the `selflow` crate does. Binary
  crates never depend on each other.
- No `unsafe` (workspace lint `unsafe_code = "forbid"`). Tests never call `std::env::set_var`.
  Code that reads the environment has a `*_from` variant that takes the values as parameters, and
  tests use that variant.
- Local store at `$XDG_STATE_HOME/selflow/selflow.db`, falling back to
  `~/.local/state/selflow/selflow.db`, directory mode 0700, file mode 0600. Socket at
  `$XDG_RUNTIME_DIR/selflow/daemon.sock`, directory mode 0700.
- Defaults from the spec: warnings 15, 5 and 1 minute before a lock; challenges are a wait, a
  written reason and an extension budget; a PIN is never a default; loosening waits 24 hours;
  tightening applies at once; a lock that already started keeps its own times until it ends, so a
  change never shortens it (spec §8.9).
- Resource targets (spec §15): the idle daemon stays near 0% CPU and in the low tens of MB of RAM,
  and nothing in the daemon ticks every second.
- Task 1 checks and declares every dependency of this milestone (the versions checked on
  2026-11-18 are in its Step 2). Later tasks don't add dependencies, and they leave `Cargo.toml`,
  `Cargo.lock` and every `lib.rs` alone. If a task truly needs a new dependency, check it first
  under the owner's library rules (`~/.claude/rules/libraries.md`) and put its one-line status in
  the commit body. The spec names `fluent`; this plan uses `fluent-bundle`, the crate `fluent`
  wraps, for its thread-safe bundle.
- Each crate's `tests/support/mod.rs` starts with `#![allow(dead_code)]`, because no single test
  file uses every helper (diario-alerr does the same).
- `make check` (`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test`) passes at the end of every task.
- Tests describe what the code does now (owner's test rules).
- Commits follow Conventional Commits, one logical change each. Only Task 16 uses `feat:`, because
  it delivers the feature; every other task uses `chore:`. Never add `Co-Authored-By` or any other
  attribution trailer.

## Review Focus

1. Suspend and resume across bedtime. Waking the laptop at 23:30 with bedtime at 23:00 locks the
   screen within 60 s, skips the stale warnings, and gives at least a minute of notice before any
   shutdown (Task 11, `locks_after_the_wall_clock_jumps_past_bedtime`).
2. The lock client dies during a lock. The daemon starts a new one within 2 s, Hyprland accepts it
   (`allow_session_lock_restore`), and a lost daemon connection never unlocks the screen (Task 12,
   `restarts_the_lock_client_when_it_dies`; Task 15, `a_lost_connection_keeps_the_screen_locked`;
   Task 16 manual check).
3. The daemon restarts in the middle of a lock. It locks again at once, keeps the night's spent
   budget and records the lock start only once (Task 11,
   `a_restart_keeps_the_spent_budget_and_one_start_event`).
4. Several monitors, including one plugged in while locked. Every output shows the lock (Task 16
   manual check).
5. A broken file in `selflow config edit`. Each error shows its line and column in the user's
   language, nothing is saved, and the editor reopens on the same text (Task 4,
   `reports_every_error_with_its_line`; Task 14, `reopens_the_editor_on_the_rejected_text`).

## Agent safety

These rules protect the owner's session and machine. They bind the controlling agent and every
subagent.

1. Never run `selflow lock`, or anything that starts it, except `selflow lock --preview`
   (Task 15). A real lock client takes the Wayland session lock at once and, by design, never
   unlocks without its daemon, so it would lock the owner out of the session they are working in.
2. When a step runs `selflowd`, set `SELFLOW_POWER=log` and point `XDG_STATE_HOME` and
   `XDG_RUNTIME_DIR` at fresh temporary directories. Never save a configuration whose lock window
   includes the current time.
3. Never run `systemctl poweroff`, `systemctl --user`, `make install` or `cargo install`, and never
   write outside the repository (dotfiles, `~/.config`, `~/.local`). Task 16 marks the steps the
   owner does.
4. Never edit the owner's Hyprland configuration. Task 16 shows the owner the lines to add.
5. Ask the owner before creating a branch or a git worktree (owner's git rules in
   `~/.claude/rules/git-conventions.md`).

## Executing this plan with subagents

Use superpowers:subagent-driven-development: one fresh implementer subagent per task, one fresh
reviewer per task, and a review of the whole branch at the end.

**What each implementer receives:** the full text of its task, the Global Constraints, the Agent
safety rules, and the paths of this plan and of the spec, with the spec sections listed for its
task in the table below. A task's Interfaces block names everything it uses from earlier tasks,
and the test fixtures it needs already exist in `crates/<crate>/tests/support/mod.rs` when it
starts.

**Shared files.** Task 1 declares every dependency and creates every module as an empty file, and
Task 2 makes the translation catalog pick up new `.ftl` files by itself. From then on, tasks in the
same wave touch different files, and no task edits `Cargo.toml`, `Cargo.lock` or a `lib.rs`. A
file a task lists under "Create" may already exist as one of Task 1's empty modules; the task
fills it.

**Order.** Running the tasks one at a time, in the wave order below, always works. To go faster,
the tasks of one wave can run in parallel, each in its own git worktree
(superpowers:using-git-worktrees), merged back into `main` in task-number order once each review
passes. Ask the owner before creating the worktrees (Agent safety rule 5). Task numbers follow the
order the plan was written in; the waves give the order to run them.

| Task | Wave | Needs | Spec sections | Test fixtures |
|---|---|---|---|---|
| 1. Workspace | 0 | | §19 | |
| 2. Translations | 1 | 1 | §16 | |
| 3. Time vocabulary | 1 | 1 | §8.2, §8.9 | creates core `brt`, `at` |
| 9. Local protocol | 1 | 1 | §6.1 | |
| 4. Configuration | 2 | 2, 3 | §8.2, §8.5, §8.8, §8.11 | adds core `VALID`, `no_jitter` |
| 14. Command line | 2 | 2, 9 | §8.9, §8.11 | creates selflow `brt`, `at`, `ScriptedEditor`, `invalid`, `saved` |
| 5. Lock windows | 3 | 4 | §8.2, §8.8 | uses core `VALID`, `brt`, `at` |
| 8. Versions | 3 | 4 | §8.9 | uses core `no_jitter`, `at` |
| 15. Lock screen | 3 | 14 | §4, §8.5 | adds selflow `view` |
| 6. Lock state | 4 | 5 | §8.3, §8.5, §8.9 | adds core `one_extension` |
| 7. Challenges | 5 | 6 | §8.5 | uses core `no_jitter`, `at`, `one_extension` |
| 10. Local store | 5 | 6, 8 | §6.7 | creates selflowd `brt`, `at`, `no_jitter`, `extension` |
| 11. Engine | 6 | 10 | §8.3, §15 | adds selflowd `TestClock`, `FakeEnforcer`, `setup`, `engine_on` |
| 12. Enforcer | 7 | 11 | §6.1, §8.3, §8.7 | adds selflowd `sample_view`, `wait_for` |
| 13. Daemon wiring | 8 | 7, 12 | §6.1, §8.9, §8.11 | adds selflowd `start`, `start_with_config`, `subscribe` |
| 16. Install and check | 9 | all | §6.1, §15 | the owner takes part |

---

### Task 1: Workspace, repository and tooling

**Files:**
- Create: `Cargo.toml`, `.prototools`, `Makefile`, `.gitignore`, `README.md`, `CLAUDE.md`
- Create: `.githooks/commit-msg`, `.githooks/pre-commit`, `.github/workflows/ci.yml`
- Create: `crates/selflow-core/`, `crates/selflow-i18n/`, `crates/selflow-protocol/`, each with
  `Cargo.toml` and `src/lib.rs`
- Create: `crates/selflowd/`, `crates/selflow/`, each with `Cargo.toml`, `src/main.rs` and
  `src/lib.rs`
- Create: every module file listed in Step 3, empty

**Interfaces:**
- Produces: library crates `selflow_core`, `selflow_i18n`, `selflow_protocol`; binaries `selflowd`
  and `selflow`, each also a library so integration tests reach their modules; every module of
  the milestone, declared and empty.

- [ ] **Step 1: Initialize the repository** in `selflow/`, which already holds `docs/`:
  `git init -b main`, then commit the existing documents on their own:
  `git add docs && git commit -m "docs: add the selflow design and plans"`.
- [ ] **Step 2: Check and declare every dependency.** Check each crate below on crates.io and in its
  repository under the owner's rules (`~/.claude/rules/libraries.md`): none may be deprecated,
  archived or yanked. Then write the root `Cargo.toml`: `[workspace]` with
  `members = ["crates/*"]` and `resolver = "3"`; `[workspace.package]` with `version = "0.1.0"`,
  `edition = "2024"` and `publish = false`; `[workspace.lints.rust]` with
  `unsafe_code = "forbid"`; and exactly this table (statuses checked on 2026-11-18):

```toml
[workspace.dependencies]
selflow-core = { path = "crates/selflow-core" }
selflow-i18n = { path = "crates/selflow-i18n" }
selflow-protocol = { path = "crates/selflow-protocol" }
clap = { version = "4.6", features = ["derive"] }
fastrand = "2.5"
fluent-bundle = "0.16"
iced = { version = "0.14", features = ["tokio"] }
iced_exwlshell = "0.21.4"
jiff = { version = "1.0", features = ["serde"] }
notify-rust = "4.19"
rusqlite = { version = "0.43", features = ["bundled"] }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.154"
tempfile = "3.27"
thiserror = "2.0.21"
tokio = { version = "1.54", features = ["fs", "io-util", "macros", "net", "process", "rt-multi-thread", "signal", "sync", "time"] }
toml = "1.2"
tracing = "0.1.44"
tracing-subscriber = "0.3.23"
unic-langid = "0.9.6"
```

- [ ] **Step 3: Crates and empty modules.** Every crate sets `version.workspace = true`,
  `edition.workspace = true`, `publish.workspace = true` and `[lints] workspace = true`, and takes
  its dependencies with `.workspace = true`. Each `lib.rs` declares the modules below as `pub mod`;
  each module is an empty `src/<name>/mod.rs`, except `app`, an empty `src/app.rs`. Both
  `main.rs` files hold an empty `fn main()` until Tasks 13 and 14.

| Crate | Dependencies | Dev-dependencies | Modules |
|---|---|---|---|
| `selflow-core` | jiff, serde, toml, fastrand, thiserror | selflow-i18n | `timing`, `config`, `plan`, `enforce`, `challenge`, `versions` |
| `selflow-i18n` | fluent-bundle, unic-langid, tracing | | `language`, `localizer`, `catalog` |
| `selflow-protocol` | serde, serde_json, jiff, tokio | | `ipc` |
| `selflowd` | selflow-core, selflow-i18n, selflow-protocol, rusqlite, tokio, tracing, tracing-subscriber, notify-rust, fastrand, serde, serde_json, jiff, thiserror | tempfile | `store`, `clock`, `engine`, `enforcer`, `ipc`, `app` |
| `selflow` | selflow-i18n, selflow-protocol, clap, iced, iced_exwlshell, tokio, tempfile, jiff, serde_json, thiserror, tracing, tracing-subscriber | | `cli`, `client`, `render`, `editor`, `lock` |

- [ ] **Step 4: Tooling files.** `.prototools` holds `rust = "~1"` and the same `[settings]` block
  as `/home/talinux/Workspace/github.com/ItzTas/personal-projects/mime-guardian/.prototools`.
  `Makefile` is `/home/talinux/Workspace/github.com/ItzTas/personal-projects/diario-alerr/Makefile`
  without the `up` and `down` targets. `.gitignore` holds `/target`. `.githooks/commit-msg` and
  `.githooks/pre-commit` are copied unchanged from
  `/home/talinux/Workspace/github.com/ItzTas/personal-projects/diario-alerr/.githooks/` and made
  executable, then `git config core.hooksPath .githooks`.
- [ ] **Step 5: CI.** `.github/workflows/ci.yml` runs on push and pull request on
  `ubuntu-latest`: checkout, Rust stable with `rustfmt` and `clippy`,
  `sudo apt-get install -y libxkbcommon-dev libwayland-dev` for iced, then `make check`.
- [ ] **Step 6: `CLAUDE.md`**, short: English everywhere; no comments; the layout rules from Global
  Constraints; Conventional Commits enforced by the hook; check every dependency on crates.io
  before adding it; run `make check` before calling work done; read the spec and the roadmap
  before planning.
- [ ] **Step 7: `README.md`.** One paragraph on what Selflow is, in the generic wording of spec §1,
  the status line "Milestone 1 in progress", and `cargo build --workspace`. Run the owner's
  humanizer skill on the prose.
- [ ] **Step 8: Verify.** `cargo build --workspace` succeeds and writes `Cargo.lock`, `make check`
  passes, and `git commit --allow-empty -m "bad message"` is rejected by the hook with exit code 1.
- [ ] **Step 9: Commit** `chore: set up the selflow workspace`, with one status line per
  dependency in the body.

### Task 2: Translations (`selflow-i18n`)

**Files:**
- Create: `crates/selflow-i18n/build.rs`, `src/language/mod.rs`, `src/localizer/mod.rs`,
  `src/localizer/types.rs`, `src/catalog/mod.rs`
- Create: `locales/en/common.ftl`, `locales/pt-BR/common.ftl`
- Test: `crates/selflow-i18n/tests/localizer.rs`, `crates/selflow-i18n/tests/parity.rs`

**Interfaces:**
- Produces:
  - `pub enum Language { En, PtBr }` with `from_tag(tag: &str) -> Option<Language>` (the language
    part before `-`, `_` or `.`, case-insensitive: `en` gives En, `pt` gives PtBr),
    `tag(self) -> &'static str` (`"en"`, `"pt-BR"`), `from_env() -> Language`, and
    `from_env_values(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Language`
    (the first non-empty value decides; when it doesn't map, En).
  - `pub struct Localizer` (`Send + Sync`) with `new(language: Language) -> Localizer`,
    `language(&self) -> Language` and `text(&self, id: &str, args: &[(&str, Arg)]) -> String`;
    `pub enum Arg { Text(String), Number(i64) }`. A missing id returns the id itself and logs a
    `tracing::warn!`.
  - `pub fn catalog(language: Language) -> &'static [(&'static str, &'static str)]`: file name and
    contents of every `.ftl` file of that language. `build.rs` generates the list into `OUT_DIR`
    from every `locales/<lang>/*.ftl` (each read with `include_str!`) and prints
    `cargo:rerun-if-changed=../../locales`, so later tasks only add `.ftl` files and never edit
    the catalog code.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/localizer.rs
#[test]
fn renders_a_message_with_arguments_in_each_language() {
    let time = || [("time", Arg::Text("07:00".into()))];
    assert_eq!(Localizer::new(Language::En).text("lock-until", &time()), "Locked until 07:00");
    assert_eq!(Localizer::new(Language::PtBr).text("lock-until", &time()), "Bloqueado até 07:00");
}

#[test]
fn maps_locale_values_to_a_language() {
    assert_eq!(Language::from_tag("pt_BR.UTF-8"), Some(Language::PtBr));
    assert_eq!(Language::from_tag("en_US.UTF-8"), Some(Language::En));
    assert_eq!(Language::from_tag("de_DE.UTF-8"), None);
    assert_eq!(Language::from_env_values(None, Some(""), Some("pt_BR.UTF-8")), Language::PtBr);
    assert_eq!(Language::from_env_values(None, None, Some("de_DE.UTF-8")), Language::En);
}

#[test]
fn returns_the_id_of_a_missing_message() {
    assert_eq!(Localizer::new(Language::En).text("no-such-message", &[]), "no-such-message");
}
```

```rust
// tests/parity.rs
fn ids(language: Language) -> BTreeMap<&'static str, BTreeSet<String>> {
    catalog(language)
        .iter()
        .map(|(file, text)| {
            let ids = text
                .lines()
                .filter_map(|line| line.split_once(" =").map(|(id, _)| id))
                .filter(|id| {
                    id.starts_with(|c: char| c.is_ascii_lowercase())
                        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                })
                .map(str::to_owned)
                .collect();
            (*file, ids)
        })
        .collect()
}

#[test]
fn every_message_exists_in_both_languages() {
    assert_eq!(ids(Language::En), ids(Language::PtBr));
}
```

- [ ] **Step 2: Run** `cargo test -p selflow-i18n`. Expected: FAIL, the items don't exist.
- [ ] **Step 3: Implement.** Bundles are `fluent_bundle::concurrent::FluentBundle` with
  `set_use_isolating(false)`, so terminal output carries no Unicode isolation marks. Each
  `Localizer` builds its bundle once from `catalog(language)`. `common.ftl` starts with
  `lock-until`.
- [ ] **Step 4: Run** `cargo test -p selflow-i18n`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: add the translation catalog`.

### Task 3: Time vocabulary (`selflow-core::timing`)

**Files:**
- Create: `crates/selflow-core/src/timing/mod.rs`, `src/timing/types.rs`
- Create: `crates/selflow-core/tests/support/mod.rs` (shared fixtures, filled in Tasks 3 to 7)
- Test: `crates/selflow-core/tests/timing.rs`

**Interfaces:**
- Produces:
  - `pub fn parse_duration(text: &str) -> Result<SignedDuration, TimingError>`: jiff's friendly
    format (`"15m"`, `"2h 30m"`, `"24h"`). Zero or negative gives `NotPositive`; anything jiff
    rejects, day units included, gives `Invalid`.
  - `pub fn parse_clock(text: &str) -> Result<civil::Time, TimingError>`:
    `civil::Time::strptime("%H:%M", text)`.
  - `pub fn logical_date(now: &Zoned, day_starts_at: civil::Time) -> civil::Date`: the civil date,
    minus one day when `now` is earlier than `day_starts_at`.
  - `pub fn instant_on(date: civil::Date, at: civil::Time, day_starts_at: civil::Time, tz: &TimeZone) -> Zoned`:
    `at` on the logical date `date`. A time earlier than `day_starts_at` falls on the next civil
    date. DST gaps use jiff's compatible disambiguation.
  - `pub fn next_after(after: &Zoned, at: civil::Time) -> Zoned`: the first instant strictly after
    `after` whose civil time is `at`.
  - `pub struct Seed(pub u64)` (Copy, Eq, Debug).
  - `pub fn jitter(seed: Seed, key: &str, date: civil::Date, window: SignedDuration) -> SignedDuration`:
    whole minutes in `[0, window]`, drawn with `fastrand::Rng::with_seed(fnv1a64(bytes))`, where
    `bytes` is `seed.0.to_le_bytes()` followed by `key` and `date.to_string()`.
  - `pub fn fnv1a64(bytes: &[u8]) -> u64` (offset basis `0xcbf29ce484222325`, prime
    `0x100000001b3`).
  - `pub enum TimingError { Invalid(String), NotPositive(String) }`.
  - In `tests/support/mod.rs`: `pub fn brt() -> TimeZone` (`TimeZone::fixed(tz::offset(-3))`, so
    tests don't need a time zone database) and
    `pub fn at(day: i8, hour: i8, minute: i8) -> Zoned` (that moment in November 2026 in `brt()`).
    2026-11-19 is a Thursday.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn parses_durations_in_the_friendly_format() {
    assert_eq!(parse_duration("15m").unwrap(), SignedDuration::from_mins(15));
    assert_eq!(parse_duration("2h 30m").unwrap(), SignedDuration::from_mins(150));
    assert!(matches!(parse_duration("0m"), Err(TimingError::NotPositive(_))));
    assert!(matches!(parse_duration("1 day"), Err(TimingError::Invalid(_))));
    assert!(matches!(parse_duration("soon"), Err(TimingError::Invalid(_))));
}

#[test]
fn parses_clock_times() {
    assert_eq!(parse_clock("23:00").unwrap(), time(23, 0, 0, 0));
    assert!(parse_clock("25:00").is_err());
}

#[test]
fn early_hours_belong_to_the_previous_logical_day() {
    let starts = time(5, 0, 0, 0);
    assert_eq!(logical_date(&at(20, 1, 30), starts), date(2026, 11, 19));
    assert_eq!(logical_date(&at(19, 23, 0), starts), date(2026, 11, 19));
}

#[test]
fn places_times_before_the_day_start_on_the_next_civil_date() {
    let instant = instant_on(date(2026, 11, 19), time(0, 30, 0, 0), time(5, 0, 0, 0), &brt());
    assert_eq!(instant, at(20, 0, 30));
}

#[test]
fn jitter_is_stable_within_a_day_and_stays_in_the_window() {
    let window = SignedDuration::from_mins(15);
    let thursday = jitter(Seed(7), "weeknight/bedtime", date(2026, 11, 19), window);
    assert_eq!(thursday, jitter(Seed(7), "weeknight/bedtime", date(2026, 11, 19), window));
    assert!(thursday >= SignedDuration::ZERO && thursday <= window);
    assert_eq!(thursday.as_secs() % 60, 0);
    let month: BTreeSet<_> = (1..=28)
        .map(|day| jitter(Seed(7), "weeknight/bedtime", date(2026, 2, day), window))
        .collect();
    assert!(month.len() > 1);
}

#[test]
fn fnv1a_matches_the_reference_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
    assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
}
```

- [ ] **Step 2: Run** `cargo test -p selflow-core --test timing`. Expected: FAIL.
- [ ] **Step 3: Implement** the functions above in `src/timing/mod.rs`.
- [ ] **Step 4: Run** `cargo test -p selflow-core --test timing`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: add time parsing, logical days and jitter`.

### Task 4: Configuration file (`selflow-core::config`)

**Files:**
- Create: `crates/selflow-core/src/config/mod.rs`, `raw.rs`, `types.rs`, `validate.rs`,
  `error.rs`, `constants.rs`
- Create: `locales/en/config.ftl`, `locales/pt-BR/config.ftl`, `locales/en/default-config.toml`,
  `locales/pt-BR/default-config.toml`
- Modify: `crates/selflow-i18n/src/catalog/mod.rs` (add `default_config`),
  `crates/selflow-core/tests/support/mod.rs` (add `VALID` and `no_jitter`)
- Test: `crates/selflow-core/tests/config.rs`

**Interfaces:**
- Consumes: `parse_duration`, `parse_clock` (Task 3). `selflow-i18n` is a dev-dependency only, for
  the default-file test.
- Produces (all types derive `Debug, Clone, PartialEq, Eq`; `Challenges` also derives serde
  `Serialize` and `Deserialize`, using jiff's `serde` feature):
  - `pub fn parse(text: &str) -> Result<Config, Vec<ConfigError>>`, errors sorted by line, then
    column.
  - `pub struct Config { pub language: String, pub day_starts_at: civil::Time, pub loosening_delay: SignedDuration, pub rule_sets: Vec<RuleSet> }`
    with `rule_set_for(&self, day: Weekday) -> Option<&RuleSet>` and `empty() -> Config` (no rule
    sets, defaults elsewhere). Anchors are resolved while parsing and not kept.
  - `pub struct RuleSet { pub name: String, pub days: BTreeSet<Weekday>, pub challenges: Challenges, pub locks: Vec<LockRule> }`
  - `pub struct Challenges { pub wait: SignedDuration, pub reason: bool, pub phrase: Option<String>, pub extensions: u32, pub extension_length: SignedDuration }`
  - `pub struct LockRule { pub id: String, pub label: String, pub start: civil::Time, pub start_jitter: SignedDuration, pub end: civil::Time, pub warnings: Vec<SignedDuration>, pub shutdown_after: Option<SignedDuration> }`
    (warnings sorted longest first)
  - `pub struct ConfigError { pub id: &'static str, pub args: Vec<(&'static str, String)>, pub line: usize, pub column: usize }`
    (1-based)
  - `pub const SUPPORTED_LANGUAGES: [&str; 2] = ["en", "pt-BR"];`
  - In `selflow-i18n`: `pub fn default_config(language: Language) -> &'static str`.

**File format.** Every key is optional unless marked required.

| Key | Value | Default |
|---|---|---|
| `language` | `"en"` or `"pt-BR"` | `"en"` |
| `day_starts_at` | clock time | `"05:00"` |
| `loosening_delay` | duration | `"24h"` |
| `[anchors]` | `name = "HH:MM"` | none |
| `[[rule_set]]` | `name` (required), `days` (required, `mon` to `sun`) | |
| `[rule_set.challenges]` | `wait`, `reason`, `phrase`, `extensions`, `extension_length` | `"2m"`, `true`, none (an empty string also means none), `1`, `"20m"` |
| `[[rule_set.lock]]` | `id` (required), `label`, `start` (required: anchor name or clock time), `start_jitter` (may be `"0m"`), `end` (required), `warnings`, `shutdown_after` | label is the id, jitter 0, warnings `["15m", "5m", "1m"]`, no shutdown |

**Error ids**, each at the position of the offending value: `config-syntax` {detail} (TOML errors,
unknown keys included), `config-bad-duration` {value}, `config-not-positive` {value},
`config-bad-time` {value}, `config-unknown-anchor` {name}, `config-unknown-language` {value},
`config-bad-day` {value}, `config-day-in-two-sets` {day, first, second}, `config-duplicate-set`
{name}, `config-duplicate-lock` {id, set}, `config-empty-window` {id}, `config-jitter-too-long`
{id}, `config-locks-overlap` {first, second, set}. The raw structs wrap every value in
`toml::Spanned` and use `#[serde(deny_unknown_fields)]`; byte offsets become line and column.

**Default files.** Both languages carry the same values; comments and `label` are in the file's
language: `language` (`"en"` or `"pt-BR"`), `day_starts_at = "05:00"`, `loosening_delay = "24h"`,
anchors `bedtime = "23:00"` and `wake = "07:00"`, one rule set `every-night` with all seven days
and the default challenges written out, and one lock `bedtime` (label "Bedtime" or "Hora de
dormir") from `bedtime` to `wake` with `start_jitter = "15m"`, warnings `["15m", "5m", "1m"]` and
`shutdown_after = "30m"`. Every key has a one-line comment.

**Fixture** in `tests/support/mod.rs` (the leading newline makes `wait` line 15 and `start`
line 22), plus `pub fn no_jitter() -> String`, which is `VALID` without the `start_jitter` line:

```rust
pub const VALID: &str = r#"
language = "pt-BR"
day_starts_at = "05:00"
loosening_delay = "24h"

[anchors]
bedtime = "23:00"
wake = "07:00"

[[rule_set]]
name = "weeknight"
days = ["sun", "mon", "tue", "wed", "thu"]

[rule_set.challenges]
wait = "2m"
reason = true
extensions = 1
extension_length = "20m"

[[rule_set.lock]]
id = "bedtime"
start = "bedtime"
start_jitter = "15m"
end = "wake"
warnings = ["5m", "15m", "1m"]
shutdown_after = "30m"
"#;
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn parses_a_valid_file_and_resolves_anchors() {
    let config = parse(VALID).unwrap();
    let set = config.rule_set_for(Weekday::Monday).unwrap();
    let lock = &set.locks[0];
    assert_eq!((lock.start, lock.end), (time(23, 0, 0, 0), time(7, 0, 0, 0)));
    assert_eq!(lock.label, "bedtime");
    assert_eq!(lock.warnings, [15, 5, 1].map(SignedDuration::from_mins).to_vec());
    assert_eq!(set.challenges.phrase, None);
    assert!(config.rule_set_for(Weekday::Friday).is_none());
}

#[test]
fn reports_every_error_with_its_line() {
    let text = VALID
        .replace(r#"wait = "2m""#, r#"wait = "soon""#)
        .replace(r#"start = "bedtime""#, r#"start = "bedtme""#);
    let found: Vec<_> = parse(&text).unwrap_err().iter().map(|e| (e.id, e.line)).collect();
    assert_eq!(found, vec![("config-bad-duration", 15), ("config-unknown-anchor", 22)]);
}

#[test]
fn rejects_unknown_keys() {
    let text = VALID.replace("language = \"pt-BR\"", "language = \"pt-BR\"\ncolour = \"red\"");
    let errors = parse(&text).unwrap_err();
    assert_eq!((errors[0].id, errors[0].line), ("config-syntax", 3));
}

#[test]
fn rejects_a_day_claimed_by_two_rule_sets() {
    let text = format!("{VALID}\n[[rule_set]]\nname = \"weekend\"\ndays = [\"mon\", \"fri\"]\n");
    let errors = parse(&text).unwrap_err();
    assert_eq!(errors[0].id, "config-day-in-two-sets");
    assert_eq!(
        errors[0].args,
        vec![("day", "mon".into()), ("first", "weeknight".into()), ("second", "weekend".into())]
    );
}

#[test]
fn rejects_overlapping_locks_in_one_rule_set() {
    let text = format!("{VALID}\n[[rule_set.lock]]\nid = \"late\"\nstart = \"01:00\"\nend = \"06:00\"\n");
    assert_eq!(parse(&text).unwrap_err()[0].id, "config-locks-overlap");
}

#[test]
fn rejects_jitter_as_long_as_the_window() {
    let text = VALID.replace(r#"wake = "07:00""#, r#"wake = "23:15""#);
    assert_eq!(parse(&text).unwrap_err()[0].id, "config-jitter-too-long");
}

#[test]
fn both_default_files_describe_the_same_rules() {
    let without_labels = |mut config: Config| {
        config.rule_sets.iter_mut().flat_map(|set| &mut set.locks).for_each(|lock| lock.label.clear());
        config.rule_sets
    };
    let en = parse(default_config(Language::En)).unwrap();
    let pt = parse(default_config(Language::PtBr)).unwrap();
    assert_eq!((en.language.as_str(), pt.language.as_str()), ("en", "pt-BR"));
    assert_eq!(en.rule_sets[0].locks[0].shutdown_after, Some(SignedDuration::from_mins(30)));
    assert_eq!(without_labels(en), without_labels(pt));
}
```

- [ ] **Step 2: Run** `cargo test -p selflow-core --test config`. Expected: FAIL.
- [ ] **Step 3: Implement** `parse` with the raw structs, `validate` and the error type, the two
  default files, and the error messages in `config.ftl` for both languages.
- [ ] **Step 4: Run** `cargo test -p selflow-core --test config` and `cargo test -p selflow-i18n`.
  Expected: PASS.
- [ ] **Step 5: Commit** `chore: parse and validate the configuration file`.

### Task 5: Lock windows (`selflow-core::plan`)

**Files:**
- Create: `crates/selflow-core/src/plan/mod.rs`, `src/plan/types.rs`
- Test: `crates/selflow-core/tests/plan.rs`

**Interfaces:**
- Consumes: `Config`, `LockRule`, `Challenges` (Task 4); `Seed`, `jitter`, `instant_on`,
  `next_after`, `logical_date` (Task 3).
- Produces:
  - `pub struct LockWindow { pub rule_set: String, pub rule_id: String, pub label: String, pub logical_date: civil::Date, pub starts_at: Zoned, pub ends_at: Zoned, pub warnings: Vec<SignedDuration>, pub shutdown_after: Option<SignedDuration>, pub challenges: Challenges }`
    deriving `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`, with
    `key(&self) -> String` returning `"{rule_set}/{rule_id}"`.
  - `pub fn windows_for(config: &Config, seed: Seed, date: civil::Date, tz: &TimeZone) -> Vec<LockWindow>`:
    the locks of the rule set for `date.weekday()`, with
    `starts_at = instant_on(date, start, day_starts_at, tz) + jitter(seed, key, date, start_jitter)`
    and `ends_at = next_after(&starts_at, end)`.
  - `pub fn windows_around(config: &Config, seed: Seed, now: &Zoned) -> Vec<LockWindow>`: the
    windows of logical dates D−1 and D, where D is `logical_date(now)`, sorted by `starts_at`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_window_runs_from_the_jittered_start_to_the_next_wake() {
    let windows = windows_for(&parse(VALID).unwrap(), Seed(7), date(2026, 11, 19), &brt());
    let offset = jitter(Seed(7), "weeknight/bedtime", date(2026, 11, 19), SignedDuration::from_mins(15));
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].starts_at, at(19, 23, 0).checked_add(offset).unwrap());
    assert_eq!(windows[0].ends_at, at(20, 7, 0));
}

#[test]
fn a_day_without_a_rule_set_has_no_window() {
    assert!(windows_for(&parse(VALID).unwrap(), Seed(7), date(2026, 11, 20), &brt()).is_empty());
}

#[test]
fn the_morning_after_still_sees_the_previous_night() {
    let windows = windows_around(&parse(VALID).unwrap(), Seed(7), &at(20, 6, 0));
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].logical_date, date(2026, 11, 19));
}
```

- [ ] **Step 2: Run** `cargo test -p selflow-core --test plan`. Expected: FAIL.
- [ ] **Step 3: Implement** `windows_for` and `windows_around`.
- [ ] **Step 4: Run** `cargo test -p selflow-core --test plan`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: compute nightly lock windows`.

### Task 6: Lock state (`selflow-core::enforce`)

**Files:**
- Create: `crates/selflow-core/src/enforce/mod.rs`, `src/enforce/types.rs`
- Modify: `crates/selflow-core/tests/support/mod.rs` (add `one_extension`)
- Test: `crates/selflow-core/tests/enforce.rs`

**Interfaces:**
- Consumes: `LockWindow`, `windows_around` (Task 5).
- Produces (all derive `Debug, Clone, PartialEq, Eq`):
  - `pub struct GrantedExtension { pub rule_set: String, pub rule_id: String, pub logical_date: civil::Date, pub granted_at: Timestamp, pub until: Timestamp, pub reason: String }`
  - `#[derive(Default)] pub struct NightLog { pub extensions: Vec<GrantedExtension>, pub started: Vec<LockWindow> }`.
    `started` holds the windows that already began, as they were when they began.
  - `pub enum Enforcement { Free, Locked(ActiveLock) }`
  - `pub struct ActiveLock { pub window: LockWindow, pub locked_since: Timestamp, pub extension_available_at: Timestamp, pub extensions_left: u32, pub shutdown_at: Option<Timestamp> }`
  - `pub struct Warning { pub key: String, pub label: String, pub at: Timestamp, pub lock_at: Timestamp, pub variant: u8 }`
    (`variant` in 1 to 3, drawn like `jitter` from the seed, the key and `lock_at`)
  - `pub struct Evaluation { pub enforcement: Enforcement, pub warnings: Vec<Warning>, pub next_change_at: Option<Timestamp> }`
  - `pub fn evaluate(now: &Zoned, config: &Config, seed: Seed, log: &NightLog) -> Evaluation`

**Rules the tests pin:**
- The candidate windows are `windows_around(config, seed, now)`, except that a window in
  `log.started` with the same key and logical date replaces the computed one, and a started window
  that the configuration no longer produces still counts. This is how a lock that already started
  keeps its own times when the configuration changes.
- Locked when `now` is in `[starts_at, ends_at)` of a window and no extension of that window (same
  rule set, rule id and logical date) has `granted_at <= now < until`.
- `locked_since` is the later of `starts_at` and the latest extension `until` that is `<= now`.
- `extension_available_at = locked_since + wait`. `extensions_left` is `extensions` minus the
  extensions granted for that window, never below 0. `shutdown_at = locked_since + shutdown_after`
  when that comes before `ends_at`, otherwise `None`.
- `warnings` lists, for each window, the warnings before its next lock moment: its start, or the
  end of the running extension. Before an extension end, only offsets shorter than the extension
  count. A window that is locked right now lists no warnings, and neither does a window that has
  ended. Past and future warnings are both listed, and the engine decides which to send.
- `next_change_at` is the earliest future instant among warnings, lock starts, extension ends,
  `extension_available_at`, `shutdown_at` and lock ends.

`one_extension()` in the fixtures returns a `NightLog` with one extension of
`weeknight/bedtime` on 2026-11-19, granted at 23:05, until 23:25, reason
`"talking to my sister"`.

- [ ] **Step 1: Write the failing tests** (with `let c = parse(&no_jitter()).unwrap();`)

```rust
#[test]
fn locks_from_bedtime_until_wake() {
    let free = |now: Zoned| matches!(evaluate(&now, &c, Seed(7), &NightLog::default()).enforcement, Enforcement::Free);
    assert!(free(at(19, 22, 59)));
    assert!(free(at(20, 7, 0)));
    let Enforcement::Locked(lock) = evaluate(&at(19, 23, 0), &c, Seed(7), &NightLog::default()).enforcement else {
        panic!("expected a lock")
    };
    assert_eq!(lock.extensions_left, 1);
    assert_eq!(lock.extension_available_at, at(19, 23, 2).timestamp());
    assert_eq!(lock.shutdown_at, Some(at(19, 23, 30).timestamp()));
    assert_eq!(lock.window.ends_at, at(20, 7, 0));
}

#[test]
fn warns_fifteen_five_and_one_minute_before() {
    let evaluation = evaluate(&at(19, 22, 0), &c, Seed(7), &NightLog::default());
    let times: Vec<_> = evaluation.warnings.iter().map(|w| w.at).collect();
    assert_eq!(times, [(22, 45), (22, 55), (22, 59)].map(|(h, m)| at(19, h, m).timestamp()).to_vec());
    assert_eq!(evaluation.next_change_at, Some(at(19, 22, 45).timestamp()));
}

#[test]
fn an_extension_frees_the_screen_and_the_lock_comes_back_after_it() {
    let during = evaluate(&at(19, 23, 10), &c, Seed(7), &one_extension());
    assert!(matches!(during.enforcement, Enforcement::Free));
    let times: Vec<_> = during.warnings.iter().map(|w| w.at).collect();
    assert_eq!(times, [(23, 10), (23, 20), (23, 24)].map(|(h, m)| at(19, h, m).timestamp()).to_vec());
    let Enforcement::Locked(lock) = evaluate(&at(19, 23, 25), &c, Seed(7), &one_extension()).enforcement else {
        panic!("expected a lock")
    };
    assert_eq!(lock.locked_since, at(19, 23, 25).timestamp());
    assert_eq!(lock.extensions_left, 0);
    assert_eq!(lock.shutdown_at, Some(at(19, 23, 55).timestamp()));
}

#[test]
fn no_shutdown_when_the_lock_ends_first() {
    let short = parse(&no_jitter().replace(r#"wake = "07:00""#, r#"wake = "23:20""#)).unwrap();
    let Enforcement::Locked(lock) = evaluate(&at(19, 23, 0), &short, Seed(7), &NightLog::default()).enforcement else {
        panic!("expected a lock")
    };
    assert_eq!(lock.shutdown_at, None);
}

#[test]
fn a_started_lock_keeps_its_times_when_the_configuration_changes() {
    let started = windows_for(&c, Seed(7), date(2026, 11, 19), &brt());
    let log = NightLog { started, ..NightLog::default() };
    let later = parse(&no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#)).unwrap();
    let Enforcement::Locked(lock) = evaluate(&at(19, 23, 10), &later, Seed(7), &log).enforcement else {
        panic!("expected a lock")
    };
    assert_eq!(lock.window.ends_at, at(20, 7, 0));
    assert!(matches!(evaluate(&at(19, 23, 10), &Config::empty(), Seed(7), &log).enforcement, Enforcement::Locked(_)));
}
```

- [ ] **Step 2: Run** `cargo test -p selflow-core --test enforce`. Expected: FAIL.
- [ ] **Step 3: Implement** `evaluate` following the rules above.
- [ ] **Step 4: Run** `cargo test -p selflow-core --test enforce`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: evaluate the lock state`.

### Task 7: Unlock challenges (`selflow-core::challenge`)

**Files:**
- Create: `crates/selflow-core/src/challenge/mod.rs`, `src/challenge/types.rs`
- Test: `crates/selflow-core/tests/challenge.rs`

**Interfaces:**
- Consumes: `ActiveLock`, `GrantedExtension` (Task 6).
- Produces:
  - `pub struct ExtensionAnswer { pub reason: String, pub phrase: String }`
  - `pub enum Refusal { TooEarly { available_at: Timestamp }, NoBudget, ReasonTooShort { min_chars: usize }, PhraseMismatch }`
    (`Debug, Clone, PartialEq, Eq`)
  - `pub const MIN_REASON_CHARS: usize = 10;`
  - `pub fn request_extension(now: Timestamp, lock: &ActiveLock, answer: &ExtensionAnswer) -> Result<GrantedExtension, Refusal>`.
    Checks in this order: `TooEarly`, `NoBudget`, `ReasonTooShort` (when a reason is required;
    characters counted after trimming), `PhraseMismatch` (when a phrase is set; compared after
    trimming, collapsing runs of whitespace and ignoring case). The grant runs until
    `min(now + extension_length, ends_at)`.

- [ ] **Step 1: Write the failing tests**

```rust
fn lock_at(now: Zoned, config: &Config, log: &NightLog) -> ActiveLock {
    match evaluate(&now, config, Seed(7), log).enforcement {
        Enforcement::Locked(lock) => lock,
        Enforcement::Free => panic!("expected a lock"),
    }
}

fn answer(reason: &str, phrase: &str) -> ExtensionAnswer {
    ExtensionAnswer { reason: reason.into(), phrase: phrase.into() }
}

#[test]
fn refuses_before_the_wait_is_over() {
    let lock = lock_at(at(19, 23, 1), &parse(&no_jitter()).unwrap(), &NightLog::default());
    assert_eq!(
        request_extension(at(19, 23, 1).timestamp(), &lock, &answer("talking to my sister", "")),
        Err(Refusal::TooEarly { available_at: at(19, 23, 2).timestamp() })
    );
}

#[test]
fn refuses_a_reason_that_is_too_short() {
    let lock = lock_at(at(19, 23, 3), &parse(&no_jitter()).unwrap(), &NightLog::default());
    assert_eq!(
        request_extension(at(19, 23, 3).timestamp(), &lock, &answer("   ok    ", "")),
        Err(Refusal::ReasonTooShort { min_chars: 10 })
    );
}

#[test]
fn grants_the_extension_length_but_never_past_the_end_of_the_lock() {
    let config = parse(&no_jitter()).unwrap();
    let evening = lock_at(at(19, 23, 3), &config, &NightLog::default());
    let granted = request_extension(at(19, 23, 3).timestamp(), &evening, &answer("talking to my sister", "")).unwrap();
    assert_eq!(granted.until, at(19, 23, 23).timestamp());
    assert_eq!((granted.rule_set.as_str(), granted.rule_id.as_str()), ("weeknight", "bedtime"));
    assert_eq!(granted.logical_date, date(2026, 11, 19));
    let morning = lock_at(at(20, 6, 50), &config, &NightLog::default());
    let granted = request_extension(at(20, 6, 50).timestamp(), &morning, &answer("talking to my sister", "")).unwrap();
    assert_eq!(granted.until, at(20, 7, 0).timestamp());
}

#[test]
fn refuses_when_the_night_budget_is_spent() {
    let lock = lock_at(at(19, 23, 30), &parse(&no_jitter()).unwrap(), &one_extension());
    assert_eq!(
        request_extension(at(19, 23, 30).timestamp(), &lock, &answer("talking to my sister", "")),
        Err(Refusal::NoBudget)
    );
}

#[test]
fn accepts_the_phrase_whatever_the_case_and_spacing() {
    let text = no_jitter().replace("extensions = 1", "extensions = 1\nphrase = \"I choose to stay up\"");
    let lock = lock_at(at(19, 23, 3), &parse(&text).unwrap(), &NightLog::default());
    let now = at(19, 23, 3).timestamp();
    assert!(request_extension(now, &lock, &answer("talking to my sister", " i  choose to STAY up ")).is_ok());
    assert_eq!(request_extension(now, &lock, &answer("talking to my sister", "i choose")), Err(Refusal::PhraseMismatch));
}
```

- [ ] **Step 2: Run** `cargo test -p selflow-core --test challenge`. Expected: FAIL.
- [ ] **Step 3: Implement** `request_extension`.
- [ ] **Step 4: Run** `cargo test -p selflow-core --test challenge`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: check unlock challenges`.

### Task 8: Configuration versions (`selflow-core::versions`)

**Files:**
- Create: `crates/selflow-core/src/versions/mod.rs`, `types.rs`, `compare.rs`
- Create: `locales/en/changes.ftl`, `locales/pt-BR/changes.ftl`
- Test: `crates/selflow-core/tests/versions.rs`

**Interfaces:**
- Consumes: `Config` and its parts (Task 4).
- Produces:
  - `pub enum ChangeKind { Tighten, Loosen, Neutral }` (`Copy, Debug, PartialEq, Eq`)
  - `pub struct Change { pub id: &'static str, pub args: Vec<(&'static str, String)>, pub kind: ChangeKind }`.
    `id` is a message id in `changes.ftl`.
  - `pub struct Classification { pub kind: ChangeKind, pub changes: Vec<Change> }`. `kind` is
    Loosen when any change loosens, else Tighten when any tightens, else Neutral.
  - `pub fn classify(current: &Config, proposed: &Config) -> Classification`
  - `pub fn effective_at(kind: ChangeKind, saved_at: Timestamp, current: &Config) -> Timestamp`:
    `saved_at` for Tighten and Neutral, `saved_at + current.loosening_delay` for Loosen. The delay
    in force counts, not the proposed one.
  - `pub struct StoredVersion { pub id: i64, pub saved_at: Timestamp, pub effective_at: Timestamp, pub text: String }`
  - `pub fn in_force(versions: &[StoredVersion], now: Timestamp) -> Option<&StoredVersion>`: the
    latest `effective_at <= now`, ties going to the higher id.
  - `pub fn pending(versions: &[StoredVersion], now: Timestamp) -> Option<&StoredVersion>`: the
    newest with `effective_at > now`.

**Comparison.** For each weekday, compare the rule set that applies (`rule_set_for`) in both
configs, matching locks by id, plus the global settings. "Later" and "earlier" mean minutes after
each config's own `day_starts_at`, so 01:00 is later than 23:00 when the day starts at 05:00. A
lock's start for comparison is `start + start_jitter`, its latest possible start. Changes with the
same id and rule on several weekdays merge into one `Change`. Args: `rule`, `days` (short tags in
Monday-first order joined with `", "`), and `old` and `new` where a value changed.

| Kind | Change ids |
|---|---|
| Tighten | `change-lock-added`, `change-lock-starts-earlier`, `change-lock-ends-later`, `change-shutdown-added`, `change-shutdown-sooner`, `change-wait-longer`, `change-reason-required`, `change-phrase-added`, `change-fewer-extensions`, `change-shorter-extensions`, `change-delay-longer` |
| Loosen | `change-lock-removed`, `change-lock-starts-later`, `change-lock-ends-earlier`, `change-shutdown-removed`, `change-shutdown-later`, `change-wait-shorter`, `change-reason-dropped`, `change-phrase-dropped`, `change-phrase-changed`, `change-more-extensions`, `change-longer-extensions`, `change-day-start`, `change-delay-shorter` |
| Neutral | `change-warnings`, `change-label`, `change-language` |

- [ ] **Step 1: Write the failing tests** (with `let current = parse(&no_jitter()).unwrap();` and
  `let with = |from: &str, to: &str| parse(&no_jitter().replace(from, to)).unwrap();`)

```rust
#[test]
fn moving_bedtime_later_loosens() {
    let result = classify(&current, &with(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#));
    assert_eq!(result.kind, ChangeKind::Loosen);
    assert_eq!(result.changes[0].id, "change-lock-starts-later");
    assert_eq!(
        result.changes[0].args,
        vec![
            ("rule", "bedtime".into()),
            ("days", "mon, tue, wed, thu, sun".into()),
            ("old", "23:00".into()),
            ("new", "01:00".into()),
        ]
    );
}

#[test]
fn moving_bedtime_earlier_tightens_and_applies_at_once() {
    let result = classify(&current, &with(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#));
    assert_eq!(result.kind, ChangeKind::Tighten);
    let saved = at(18, 10, 0).timestamp();
    assert_eq!(effective_at(result.kind, saved, &current), saved);
}

#[test]
fn a_loosening_waits_the_delay_in_force_even_when_the_file_shortens_it() {
    let proposed = parse(&no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#).replace(r#"loosening_delay = "24h""#, r#"loosening_delay = "1h""#)).unwrap();
    let result = classify(&current, &proposed);
    assert!(result.changes.iter().any(|c| c.id == "change-delay-shorter"));
    assert_eq!(effective_at(result.kind, at(18, 10, 0).timestamp(), &current), at(19, 10, 0).timestamp());
}

#[test]
fn a_mixed_change_counts_as_loosening() {
    let proposed = parse(&no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "22:00""#).replace("extensions = 1", "extensions = 3")).unwrap();
    assert_eq!(classify(&current, &proposed).kind, ChangeKind::Loosen);
}

#[test]
fn removing_a_day_loosens() {
    let result = classify(&current, &with(r#"days = ["sun", "mon", "tue", "wed", "thu"]"#, r#"days = ["mon", "tue", "wed", "thu"]"#));
    assert_eq!((result.kind, result.changes[0].id), (ChangeKind::Loosen, "change-lock-removed"));
    assert_eq!(result.changes[0].args[1], ("days", "sun".into()));
}

#[test]
fn rewording_labels_and_warnings_is_neutral() {
    let proposed = parse(&no_jitter().replace("id = \"bedtime\"", "id = \"bedtime\"\nlabel = \"Sleep\"").replace(r#"warnings = ["5m", "15m", "1m"]"#, r#"warnings = ["10m"]"#)).unwrap();
    assert_eq!(classify(&current, &proposed).kind, ChangeKind::Neutral);
}

#[test]
fn the_first_configuration_applies_at_once() {
    assert_eq!(classify(&Config::empty(), &current).kind, ChangeKind::Tighten);
}

#[test]
fn finds_the_version_in_force_and_the_pending_one() {
    let version = |id, effective: Zoned| StoredVersion { id, saved_at: at(18, 10, 0).timestamp(), effective_at: effective.timestamp(), text: String::new() };
    let versions = vec![version(1, at(18, 10, 0)), version(2, at(19, 10, 0))];
    assert_eq!(in_force(&versions, at(18, 12, 0).timestamp()).map(|v| v.id), Some(1));
    assert_eq!(pending(&versions, at(18, 12, 0).timestamp()).map(|v| v.id), Some(2));
    assert_eq!(in_force(&versions, at(19, 10, 0).timestamp()).map(|v| v.id), Some(2));
    assert_eq!(pending(&versions, at(19, 10, 0).timestamp()).map(|v| v.id), None);
}
```

- [ ] **Step 2: Run** `cargo test -p selflow-core --test versions`. Expected: FAIL.
- [ ] **Step 3: Implement** `classify`, `effective_at`, `in_force` and `pending`, and write every
  change message in `changes.ftl` for both languages.
- [ ] **Step 4: Run** `cargo test -p selflow-core --test versions` and
  `cargo test -p selflow-i18n`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: classify configuration changes`.

### Task 9: Local protocol (`selflow-protocol`)

**Files:**
- Create: `crates/selflow-protocol/src/ipc/mod.rs`, `src/ipc/types.rs`, `src/ipc/codec.rs`
- Test: `crates/selflow-protocol/tests/ipc.rs`

**Interfaces:**
- Produces (enums use `#[serde(tag = "type", rename_all = "snake_case")]`; everything derives
  `Debug, Clone, PartialEq, Serialize, Deserialize`; time values are `jiff::Timestamp` and
  `jiff::civil::Date`):
  - `pub enum Request { Status, SubscribeLock, RequestExtension { reason: String, phrase: String }, GetConfig, SaveConfig { text: String }, ConfigStatus, Report { logical_date: Option<civil::Date> } }`
  - `pub enum Response { Status(StatusView), Lock(LockView), LockEnded, ExtensionGranted { until: Timestamp }, ExtensionRefused(RefusalView), Config { text: String }, Saved { kind: ChangeKindView, effective_at: Timestamp, changes: Vec<MessageView> }, Invalid { errors: Vec<PositionedMessage> }, ConfigStatus(ConfigStatusView), Report(ReportView), Error(MessageView) }`
  - `pub struct MessageView { pub id: String, pub args: Vec<(String, String)> }`
  - `pub struct PositionedMessage { pub message: MessageView, pub line: usize, pub column: usize }`
  - `pub struct StatusView { pub language: String, pub locked: Option<LockView>, pub next_lock_at: Option<Timestamp> }`
  - `pub struct LockView { pub label: String, pub ends_at: Timestamp, pub extension_available_at: Timestamp, pub extensions_left: u32, pub extension_minutes: i64, pub reason_required: bool, pub phrase: Option<String>, pub shutdown_at: Option<Timestamp>, pub language: String }`
  - `pub enum RefusalView { TooEarly { available_at: Timestamp }, NoBudget, ReasonTooShort { min_chars: usize }, PhraseMismatch }`
  - `pub enum ChangeKindView { Tighten, Loosen, Neutral }`
  - `pub struct ConfigStatusView { pub in_force_saved_at: Option<Timestamp>, pub pending: Option<PendingView> }`
  - `pub struct PendingView { pub saved_at: Timestamp, pub effective_at: Timestamp, pub changes: Vec<MessageView> }`
  - `pub struct ReportView { pub logical_date: civil::Date, pub entries: Vec<ReportEntry> }`
  - `pub struct ReportEntry { pub at: Timestamp, pub message: MessageView }`
  - `pub const MAX_LINE_BYTES: usize = 1 << 20;`
  - `pub async fn write_message<T: Serialize>(writer: &mut (impl AsyncWrite + Unpin), message: &T) -> io::Result<()>`:
    one JSON object followed by `\n`.
  - `pub async fn read_message<T: DeserializeOwned>(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Option<T>>`:
    `None` at end of input; a line over `MAX_LINE_BYTES` gives `ErrorKind::InvalidData`.
  - `pub fn socket_path_from(runtime_dir: Option<&OsStr>) -> Option<PathBuf>` and
    `pub fn socket_path() -> Option<PathBuf>`: `<runtime dir>/selflow/daemon.sock`.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn round_trips_messages_as_json_lines() {
    let (mut writer, other_end) = tokio::io::duplex(64 * 1024);
    let mut reader = tokio::io::BufReader::new(other_end);
    let sent = Request::SaveConfig { text: "a\nb".into() };
    write_message(&mut writer, &sent).await.unwrap();
    assert_eq!(read_message::<Request>(&mut reader).await.unwrap(), Some(sent));
}

#[test]
fn tags_messages_with_a_snake_case_type() {
    assert_eq!(serde_json::to_string(&Request::Status).unwrap(), r#"{"type":"status"}"#);
}

#[tokio::test]
async fn rejects_a_line_over_the_limit() {
    let line = format!("\"{}\"\n", "x".repeat(MAX_LINE_BYTES + 1));
    let mut reader = tokio::io::BufReader::new(line.as_bytes());
    let error = read_message::<Request>(&mut reader).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn the_socket_lives_under_the_runtime_dir() {
    assert_eq!(socket_path_from(Some("/run/user/1000".as_ref())), Some("/run/user/1000/selflow/daemon.sock".into()));
    assert_eq!(socket_path_from(None), None);
}
```

- [ ] **Step 2: Run** `cargo test -p selflow-protocol`. Expected: FAIL.
- [ ] **Step 3: Implement** the types and the codec.
- [ ] **Step 4: Run** `cargo test -p selflow-protocol`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: define the local socket protocol`.

### Task 10: Local store (`selflowd::store`)

**Files:**
- Create: `crates/selflowd/src/store/mod.rs`, `src/store/migrations.rs`, `src/store/types.rs`
- Create: `crates/selflowd/tests/support/mod.rs` (`brt`, `at`, `no_jitter` and an `extension`
  builder, mirroring the core fixtures)
- Test: `crates/selflowd/tests/store.rs`

**Interfaces:**
- Consumes: `StoredVersion`, `ChangeKind` (Task 8); `GrantedExtension`, `NightLog` (Task 6);
  `LockWindow` (Task 5); `Seed` (Task 3).
- Produces:
  - `pub struct Store` around a `rusqlite::Connection` (bundled SQLite), with:
    - `open(path: &Path) -> Result<Store, StoreError>`: creates the parent directory with mode
      0700 and the file with mode 0600 before SQLite opens it, then migrates.
    - `open_in_memory() -> Result<Store, StoreError>`
    - `seed(&self) -> Result<Seed, StoreError>`: created once with `fastrand::u64(..)` and kept.
    - `save_version(&self, text: &str, saved_at: Timestamp, effective_at: Timestamp, kind: ChangeKind) -> Result<i64, StoreError>`:
      marks every version with `effective_at > saved_at` as superseded before inserting.
    - `versions(&self) -> Result<Vec<StoredVersion>, StoreError>`: versions not superseded, by id.
    - `latest_text(&self) -> Result<Option<String>, StoreError>`: the newest version not
      superseded, pending included.
    - `record_extension(&self, extension: &GrantedExtension) -> Result<(), StoreError>`
    - `record_started(&self, window: &LockWindow) -> Result<(), StoreError>`: ignored when that key
      and logical date are already stored.
    - `night_log(&self, dates: &[civil::Date]) -> Result<NightLog, StoreError>`: extensions and
      started windows of those logical dates.
    - `record_event(&self, event: &Event) -> Result<bool, StoreError>`: `false` when the same kind
      and key already exist.
    - `events_for(&self, logical_date: civil::Date) -> Result<Vec<Event>, StoreError>`, ordered by
      `at`.
  - `pub struct Event { pub at: Timestamp, pub logical_date: civil::Date, pub kind: EventKind, pub key: String, pub detail: Option<String> }`
  - `pub enum EventKind { LockStarted, LockEnded, ExtensionGranted, Shutdown, ConfigSaved }`.
    `Event` and `EventKind` derive `Debug, Clone, PartialEq, Eq`.
  - `pub fn default_path_from(state_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf>`
    and `pub fn default_path() -> Option<PathBuf>`.

**Schema** (migrations keyed by `PRAGMA user_version`, as a const array in `migrations.rs`;
timestamps as RFC 3339 text, dates as `YYYY-MM-DD`, kinds as snake_case text):

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
CREATE TABLE config_versions (id INTEGER PRIMARY KEY, saved_at TEXT NOT NULL, effective_at TEXT NOT NULL,
  kind TEXT NOT NULL, text TEXT NOT NULL, superseded INTEGER NOT NULL DEFAULT 0) STRICT;
CREATE TABLE extensions (id INTEGER PRIMARY KEY, rule_set TEXT NOT NULL, rule_id TEXT NOT NULL,
  logical_date TEXT NOT NULL, granted_at TEXT NOT NULL, until TEXT NOT NULL, reason TEXT NOT NULL) STRICT;
CREATE TABLE started_windows (key TEXT NOT NULL, logical_date TEXT NOT NULL, window TEXT NOT NULL,
  PRIMARY KEY (key, logical_date)) STRICT;
CREATE TABLE events (id INTEGER PRIMARY KEY, at TEXT NOT NULL, logical_date TEXT NOT NULL, kind TEXT NOT NULL,
  key TEXT NOT NULL, detail TEXT, UNIQUE (kind, key)) STRICT;
```

`started_windows.window` is the `LockWindow` as JSON. Event `detail` is JSON:
`{"label"}` for LockStarted, LockEnded and Shutdown, `{"label", "minutes", "reason"}` for
ExtensionGranted, `{"kind"}` for ConfigSaved.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn keeps_one_seed_for_the_life_of_the_database() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.seed().unwrap(), store.seed().unwrap());
}

#[test]
fn a_new_save_replaces_the_pending_version() {
    let store = Store::open_in_memory().unwrap();
    let ts = |day, hour| at(day, hour, 0).timestamp();
    let first = store.save_version("one", ts(18, 10), ts(18, 10), ChangeKind::Tighten).unwrap();
    store.save_version("two", ts(18, 11), ts(19, 11), ChangeKind::Loosen).unwrap();
    let third = store.save_version("three", ts(18, 12), ts(18, 12), ChangeKind::Tighten).unwrap();
    assert_eq!(store.versions().unwrap().iter().map(|v| v.id).collect::<Vec<_>>(), vec![first, third]);
    assert_eq!(store.latest_text().unwrap().as_deref(), Some("three"));
}

#[test]
fn records_an_event_only_once() {
    let store = Store::open_in_memory().unwrap();
    let event = Event {
        at: at(19, 23, 0).timestamp(),
        logical_date: date(2026, 11, 19),
        kind: EventKind::LockStarted,
        key: "weeknight/bedtime/2026-11-19/2026-11-20T02:00:00Z".into(),
        detail: Some(r#"{"label":"bedtime"}"#.into()),
    };
    assert!(store.record_event(&event).unwrap());
    assert!(!store.record_event(&event).unwrap());
    assert_eq!(store.events_for(date(2026, 11, 19)).unwrap().len(), 1);
}

#[test]
fn reads_back_the_log_of_the_requested_nights() {
    let store = Store::open_in_memory().unwrap();
    let extension = extension((23, 5), (23, 25));
    let window = windows_for(&parse(&no_jitter()).unwrap(), Seed(7), date(2026, 11, 19), &brt()).remove(0);
    store.record_extension(&extension).unwrap();
    store.record_started(&window).unwrap();
    store.record_started(&window).unwrap();
    let log = store.night_log(&[date(2026, 11, 19)]).unwrap();
    assert_eq!((log.extensions, log.started), (vec![extension], vec![window]));
    assert_eq!(store.night_log(&[date(2026, 11, 18)]).unwrap(), NightLog::default());
}

#[test]
fn creates_files_only_the_user_can_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/selflow.db");
    Store::open(&path).unwrap();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&path), mode(path.parent().unwrap())), (0o600, 0o700));
}

#[test]
fn the_default_path_follows_xdg_state_home() {
    assert_eq!(default_path_from(Some("/s".as_ref()), Some("/h".as_ref())), Some("/s/selflow/selflow.db".into()));
    assert_eq!(default_path_from(None, Some("/h".as_ref())), Some("/h/.local/state/selflow/selflow.db".into()));
}
```

- [ ] **Step 2: Run** `cargo test -p selflowd --test store`. Expected: FAIL.
- [ ] **Step 3: Implement** the store and its migrations.
- [ ] **Step 4: Run** `cargo test -p selflowd --test store`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: add the local store`.

### Task 11: Enforcement loop (`selflowd::engine`)

**Files:**
- Create: `crates/selflowd/src/clock/mod.rs`, `src/engine/mod.rs`, `src/engine/types.rs`,
  `src/engine/constants.rs`
- Create: `locales/en/warnings.ftl`, `locales/pt-BR/warnings.ftl`
- Modify: `crates/selflowd/tests/support/mod.rs` (add `TestClock` and `FakeEnforcer`)
- Test: `crates/selflowd/tests/engine.rs`

**Interfaces:**
- Consumes: `evaluate`, `ActiveLock` (Task 6); `in_force` (Task 8); `parse`, `Config` (Task 4);
  `Store`, `Event` (Task 10); `LockView` (Task 9); `Localizer` (Task 2).
- Produces:
  - `pub trait Clock: Send + Sync { fn now(&self) -> Zoned; }` and `pub struct SystemClock`
    (`Zoned::now()`).
  - `pub trait Enforcer: Send { fn warn(&mut self, title: String, body: String); fn show_lock(&mut self, view: LockView); fn hide_lock(&mut self); fn shut_down(&mut self); }`.
    `show_lock` and `hide_lock` are called on every tick and must be idempotent.
  - `pub const MAX_SLEEP: SignedDuration = SignedDuration::from_secs(60);`
    `pub const WARNING_GRACE: SignedDuration = SignedDuration::from_secs(120);`
    `pub const SHUTDOWN_NOTICE: SignedDuration = SignedDuration::from_secs(60);`
  - `pub struct Engine<C: Clock, E: Enforcer>` with
    `new(clock: Arc<C>, enforcer: E, store: Arc<Mutex<Store>>) -> Result<Self, EngineError>`,
    `tick(&mut self) -> Result<Timestamp, EngineError>` and
    `async fn run(self, wake: tokio::sync::mpsc::Receiver<()>) -> Result<(), EngineError>`.
  - `pub fn config_in_force(store: &Store, now: Timestamp) -> Result<Config, EngineError>`: the
    parsed version in force, or `Config::empty()`.
  - `pub fn lock_view(lock: &ActiveLock, language: &str) -> LockView`

**What `tick` does:**
1. Loads the config in force (cached by version id), the seed and the night log of logical dates
   D−1 and D, then calls `evaluate`.
2. Sends each warning with `now − WARNING_GRACE < at <= now` that this process hasn't sent yet
   (an in-memory set of key and time), titled `warning-title` with body `warning-body-1`, `-2` or
   `-3` by `variant`, with args `label`, `time` (local `HH:MM` of `lock_at`) and `minutes`.
3. When locked: records the window with `record_started` if the log doesn't have it, records
   `LockStarted` with key `"{window key}/{logical date}/{locked_since}"`, and calls `show_lock`.
   The view's `shutdown_at` is the later of the lock's `shutdown_at` and the moment this process
   first showed that lock plus `SHUTDOWN_NOTICE`. When that moment has passed, it records
   `Shutdown` and calls `shut_down`.
4. When free: calls `hide_lock`, and records `LockEnded` when this process saw that lock.
5. Returns `min(next_change_at, now + MAX_SLEEP)`.

`run` calls `tick`, then waits with `tokio::select!` for that time or for a message on `wake`.

- [ ] **Step 1: Write the failing tests.** `setup(now)` builds an in-memory store holding
  `no_jitter()` saved at 2026-11-18 10:00 as Tighten, a `TestClock` set to `now`, and a
  `FakeEnforcer` that records `Call::Warn(String)`, `Call::Show(String, u32)` (label and
  extensions left), `Call::Hide` and `Call::ShutDown`. `engine_on(store, clock)` builds a second
  engine over the same store and clock, as a daemon restart would.

```rust
#[test]
fn locks_at_bedtime_and_records_the_start_once() {
    let (mut engine, calls, _clock, store) = setup(at(19, 23, 0));
    engine.tick().unwrap();
    engine.tick().unwrap();
    assert_eq!(calls.take(), vec![Call::Show("bedtime".into(), 1), Call::Show("bedtime".into(), 1)]);
    assert_eq!(store.lock().unwrap().events_for(date(2026, 11, 19)).unwrap().len(), 1);
}

#[test]
fn sends_a_warning_once() {
    let (mut engine, calls, _clock, _store) = setup(at(19, 22, 45));
    engine.tick().unwrap();
    engine.tick().unwrap();
    assert_eq!(calls.take().iter().filter(|c| matches!(c, Call::Warn(_))).count(), 1);
}

#[test]
fn locks_after_the_wall_clock_jumps_past_bedtime() {
    let (mut engine, calls, clock, _store) = setup(at(19, 22, 30));
    assert_eq!(engine.tick().unwrap(), at(19, 22, 31).timestamp());
    clock.set(at(19, 23, 30));
    engine.tick().unwrap();
    clock.set(at(19, 23, 31));
    engine.tick().unwrap();
    let calls = calls.take();
    assert!(!calls.iter().any(|c| matches!(c, Call::Warn(_))));
    assert_eq!(calls[1..], [Call::Show("bedtime".into(), 1), Call::Show("bedtime".into(), 1), Call::ShutDown]);
}

#[test]
fn a_restart_keeps_the_spent_budget_and_one_start_event() {
    let (mut first, _calls, clock, store) = setup(at(19, 23, 30));
    store.lock().unwrap().record_extension(&extension((23, 5), (23, 25))).unwrap();
    first.tick().unwrap();
    drop(first);
    let (mut second, calls) = engine_on(store.clone(), clock.clone());
    second.tick().unwrap();
    assert_eq!(calls.take(), vec![Call::Show("bedtime".into(), 0)]);
    let events = store.lock().unwrap().events_for(date(2026, 11, 19)).unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::LockStarted).count(), 1);
}
```

- [ ] **Step 2: Run** `cargo test -p selflowd --test engine`. Expected: FAIL.
- [ ] **Step 3: Implement** the clock, the engine and the warning messages in `warnings.ftl`
  (three calm variants per language).
- [ ] **Step 4: Run** `cargo test -p selflowd --test engine`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: run the enforcement loop`.

### Task 12: Lock client, notifications and shutdown (`selflowd::enforcer`)

**Files:**
- Create: `crates/selflowd/src/enforcer/mod.rs`, `lock_supervisor.rs`, `notifier.rs`, `power.rs`
- Modify: `crates/selflowd/tests/support/mod.rs` (add `sample_view` and `wait_for`)
- Test: `crates/selflowd/tests/lock_supervisor.rs`

**Interfaces:**
- Consumes: `Enforcer` (Task 11), `LockView` (Task 9).
- Produces:
  - `pub struct LockSupervisor` with `new(program: PathBuf, args: Vec<String>) -> Self`,
    `show(&mut self, view: LockView)` (publishes the view and starts the client when none runs),
    `hide(&mut self)` (publishes `None` and kills the client if it still runs 5 s later),
    `views(&self) -> tokio::sync::watch::Receiver<Option<LockView>>` and
    `pid(&self) -> Option<u32>`. A watcher task waits on the child; when it exits while the
    published view is `Some`, a new client starts after `RESPAWN_DELAY` (1 s).
  - `pub fn lock_program_from(current_exe: &Path) -> PathBuf`: the `selflow` file next to the
    daemon when it exists, otherwise `selflow` from `PATH`. The client runs as `selflow lock`.
  - `pub struct Notifier` with `send(&self, title: &str, body: &str)`: notify-rust, app name
    `Selflow`, normal urgency, no sound hint. Errors are logged, never fatal.
  - `pub enum PowerMode { Real, LogOnly }`, `pub fn power_mode_from(value: Option<&OsStr>) -> PowerMode`
    (`SELFLOW_POWER=log` gives LogOnly) and `pub fn shut_down(mode: PowerMode)`: Real runs
    `systemctl poweroff`, LogOnly logs `shutdown requested` with `tracing::warn!`.
  - `pub struct SystemEnforcer` implementing `Enforcer` with these three parts.

- [ ] **Step 1: Write the failing tests** (`wait_for(limit, condition)` polls every 50 ms)

```rust
#[tokio::test]
async fn restarts_the_lock_client_when_it_dies() {
    let mut supervisor = LockSupervisor::new("sleep".into(), vec!["30".into()]);
    supervisor.show(sample_view());
    let first = supervisor.pid().expect("the client started");
    std::process::Command::new("kill").arg(first.to_string()).status().unwrap();
    assert!(wait_for(Duration::from_secs(2), || supervisor.pid().is_some_and(|pid| pid != first)).await);
    supervisor.hide();
}

#[tokio::test]
async fn hiding_publishes_the_end_and_stops_the_client() {
    let mut supervisor = LockSupervisor::new("sleep".into(), vec!["30".into()]);
    let views = supervisor.views();
    supervisor.show(sample_view());
    supervisor.hide();
    assert_eq!(*views.borrow(), None);
    assert!(wait_for(Duration::from_secs(7), || supervisor.pid().is_none()).await);
}

#[test]
fn finds_the_lock_program_next_to_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("selflow"), "").unwrap();
    assert_eq!(lock_program_from(&dir.path().join("selflowd")), dir.path().join("selflow"));
    assert_eq!(lock_program_from(Path::new("/nowhere/selflowd")), PathBuf::from("selflow"));
}

#[test]
fn log_mode_never_powers_off() {
    assert_eq!(power_mode_from(Some("log".as_ref())), PowerMode::LogOnly);
    assert_eq!(power_mode_from(None), PowerMode::Real);
}
```

- [ ] **Step 2: Run** `cargo test -p selflowd --test lock_supervisor`. Expected: FAIL.
- [ ] **Step 3: Implement** the supervisor with `tokio::process`, the notifier and the power
  switch.
- [ ] **Step 4: Run** `cargo test -p selflowd --test lock_supervisor`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: drive the lock client, notifications and shutdown`.

### Task 13: Socket server and daemon wiring (`selflowd::ipc`, `app`, `main`)

**Files:**
- Create: `crates/selflowd/src/ipc/mod.rs`, `src/ipc/handlers.rs`, `src/ipc/types.rs`,
  `src/app.rs`
- Create: `locales/en/report.ftl`, `locales/pt-BR/report.ftl`, `contrib/selflowd.service`
- Modify: `crates/selflowd/src/main.rs`
- Test: `crates/selflowd/tests/ipc.rs`

**Interfaces:**
- Consumes: everything from Tasks 4 to 12.
- Produces:
  - `pub struct Shared { pub store: Arc<Mutex<Store>>, pub clock: Arc<dyn Clock>, pub wake: mpsc::Sender<()>, pub views: watch::Receiver<Option<LockView>> }`
  - `pub fn bind(path: &Path) -> Result<UnixListener, IpcError>`: creates the directory with mode
    0700, removes a stale socket, and returns `IpcError::AlreadyRunning` when another daemon
    answers on it.
  - `pub async fn serve(listener: UnixListener, shared: Arc<Shared>) -> Result<(), IpcError>`
  - `pub fn handle(request: Request, shared: &Shared) -> Response`, for every request except
    `SubscribeLock`.
  - `pub async fn run() -> Result<(), AppError>` in `app.rs`: opens the store at `default_path()`,
    builds the `Engine` with `SystemClock` and `SystemEnforcer` (power mode from
    `SELFLOW_POWER`), binds `socket_path()`, spawns `serve`, runs the engine, and exits cleanly on
    SIGTERM.
  - `main.rs`: `fn main() -> ExitCode` that sets up `tracing-subscriber` on stderr, starts a tokio
    runtime, runs `app::run`, and turns an error into one log line and exit code 1.

**Handlers:**
- `Status`: the language of the config in force, the lock view when locked, the next lock start.
- `GetConfig`: `latest_text()`, else `default_config(Language::from_env())`.
- `SaveConfig { text }`: parse errors give `Invalid` with ids, args, line and column. Otherwise
  classify against the config in force (or `Config::empty()`), compute `effective_at`, call
  `save_version`, record `ConfigSaved`, send on `wake`, and reply
  `Saved { kind, effective_at, changes }`.
- `ConfigStatus`: `saved_at` of the version in force, and the pending version with
  `classify(in force, pending)` as its changes.
- `RequestExtension`: evaluate now. When free, `Error` with id `ipc-not-locked`. Otherwise
  `request_extension`; a grant calls `record_extension`, records `ExtensionGranted`, sends on
  `wake` and replies `ExtensionGranted { until }`; a refusal replies `ExtensionRefused`.
- `Report { logical_date }`: events of that logical date, by default the previous one, as entries
  with ids `report-lock-started` {label}, `report-lock-ended` {label}, `report-extension` {label,
  minutes, reason}, `report-shutdown` {label} and `report-config-saved-tighten`, `-loosen` or
  `-neutral`. When a version is pending, a last entry `report-pending` {when} says when it takes
  effect (spec §8.9).
- `SubscribeLock`, inside `serve`: sends `Lock(view)` for every `Some` on `views` and `LockEnded`
  for `None`, until the client disconnects.

**`contrib/selflowd.service`**, exactly:

```ini
[Unit]
Description=Selflow daemon

[Service]
ExecStart=%h/.cargo/bin/selflowd
Restart=always
RestartSec=2
```

It has no `[Install]` section on purpose: Hyprland's start hook restarts it after importing
`WAYLAND_DISPLAY` (Task 16).

- [ ] **Step 1: Write the failing tests** over a real socket in a temporary directory, with an
  in-memory store and a `TestClock`. Helpers in `tests/support`: `start(now)` returns a test
  client and the `Shared`; `start_with_config(now)` does the same with `no_jitter()` in force
  since 2026-11-18 10:00 and also returns the `TestClock`; `subscribe()` returns a subscriber and
  the `watch::Sender` that feeds `views`. The test client's `request` returns the `Response`
  itself.

```rust
#[tokio::test]
async fn an_invalid_file_is_rejected_with_positions_and_not_saved() {
    let (client, shared) = start(at(18, 10, 0)).await;
    let text = no_jitter().replace(r#"wait = "2m""#, r#"wait = "soon""#);
    let Response::Invalid { errors } = client.request(Request::SaveConfig { text }).await else { panic!("expected Invalid") };
    assert_eq!((errors[0].message.id.as_str(), errors[0].line), ("config-bad-duration", 15));
    assert_eq!(shared.store.lock().unwrap().latest_text().unwrap(), None);
}

#[tokio::test]
async fn a_loosening_save_says_when_it_takes_effect() {
    let (client, _shared) = start(at(18, 10, 0)).await;
    let first = client.request(Request::SaveConfig { text: no_jitter() }).await;
    assert!(matches!(first, Response::Saved { kind: ChangeKindView::Tighten, .. }));
    let later = no_jitter().replace(r#"bedtime = "23:00""#, r#"bedtime = "01:00""#);
    let Response::Saved { kind, effective_at, changes } = client.request(Request::SaveConfig { text: later }).await else {
        panic!("expected Saved")
    };
    assert_eq!((kind, effective_at), (ChangeKindView::Loosen, at(19, 10, 0).timestamp()));
    assert_eq!(changes[0].id, "change-lock-starts-later");
    let Response::ConfigStatus(status) = client.request(Request::ConfigStatus).await else { panic!("expected ConfigStatus") };
    assert_eq!(status.pending.map(|p| p.effective_at), Some(at(19, 10, 0).timestamp()));
}

#[tokio::test]
async fn grants_one_extension_after_the_wait() {
    let (client, _shared, clock) = start_with_config(at(19, 23, 3)).await;
    let ask = || Request::RequestExtension { reason: "talking to my sister".into(), phrase: String::new() };
    assert_eq!(client.request(ask()).await, Response::ExtensionGranted { until: at(19, 23, 23).timestamp() });
    clock.set(at(19, 23, 30));
    assert_eq!(client.request(ask()).await, Response::ExtensionRefused(RefusalView::NoBudget));
}

#[tokio::test]
async fn a_lock_subscriber_gets_the_view_then_the_end() {
    let (mut subscriber, views) = subscribe().await;
    views.send(Some(sample_view())).unwrap();
    assert_eq!(subscriber.next().await, Some(Response::Lock(sample_view())));
    views.send(None).unwrap();
    assert_eq!(subscriber.next().await, Some(Response::LockEnded));
}

#[tokio::test]
async fn a_second_daemon_cannot_take_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selflow/daemon.sock");
    let _first = bind(&path).unwrap();
    assert!(matches!(bind(&path), Err(IpcError::AlreadyRunning)));
}
```

- [ ] **Step 2: Run** `cargo test -p selflowd --test ipc`. Expected: FAIL.
- [ ] **Step 3: Implement** the server, the handlers, `app::run`, `main`, the report messages and
  the unit file.
- [ ] **Step 4: Run** `cargo test -p selflowd`. Expected: PASS.
- [ ] **Step 5: Verify the binary safely** (Agent safety rule 2). With `state=$(mktemp -d)` and
  `run=$(mktemp -d)`, start
  `XDG_STATE_HOME=$state XDG_RUNTIME_DIR=$run SELFLOW_POWER=log cargo run -p selflowd` in the
  background: `$run/selflow/daemon.sock` appears, and with no saved configuration nothing locks.
  A second run with the same variables exits with code 1 saying another daemon is running. SIGTERM
  stops the first one with exit code 0.
- [ ] **Step 6: Commit** `chore: serve the local socket and wire up the daemon`.

### Task 14: Command line (`selflow config edit|status`, `selflow report`)

**Files:**
- Create: `crates/selflow/src/cli/mod.rs`, `src/cli/types.rs`, `src/cli/commands/mod.rs`,
  `src/cli/commands/config_cmd.rs`, `src/cli/commands/report_cmd.rs`, `src/client/mod.rs`,
  `src/render/mod.rs`, `src/editor/mod.rs`
- Create: `locales/en/cli.ftl`, `locales/pt-BR/cli.ftl`
- Create: `crates/selflow/tests/support/mod.rs` (`brt`, `at`, `ScriptedEditor` with `seen()`, and
  `invalid()` and `saved()` responses; Task 15 adds `view()`)
- Modify: `crates/selflow/src/main.rs`
- Test: `crates/selflow/tests/render.rs`, `crates/selflow/tests/edit_loop.rs`

**Interfaces:**
- Consumes: the protocol (Task 9), `Localizer` (Task 2).
- Produces:
  - `#[derive(Parser)] pub struct Cli { #[command(subcommand)] pub command: Command }` with
    `arg_required_else_help`, and `pub enum Command { Config(ConfigArgs), Report(ReportArgs) }`.
    Task 15 adds the hidden `Lock` command.
  - `pub fn run() -> ExitCode` in `src/cli/mod.rs`: parses `Cli`, calls the command and turns a
    `CliError` into its message and exit code. `main` only calls it.
  - `pub struct ConfigArgs { #[command(subcommand)] pub action: ConfigAction }`,
    `pub enum ConfigAction { Edit, Status }`,
    `pub struct ReportArgs { #[arg(long)] pub date: Option<civil::Date> }`.
  - `pub fn config_cmd(args: ConfigArgs) -> Result<ExitCode, CliError>` and
    `pub fn report_cmd(args: ReportArgs) -> Result<ExitCode, CliError>`. Exit codes: 0 when done
    or unchanged, 1 when abandoned, 2 when the daemon can't be reached (`cli-daemon-unreachable`,
    in the `LANG` language).
  - `pub struct Client` with `connect(path: &Path) -> Result<Client, CliError>` and
    `request(&mut self, request: Request) -> Result<Response, CliError>`.
  - `pub fn render_errors(localizer: &Localizer, errors: &[PositionedMessage]) -> String`
  - `pub fn render_saved(localizer: &Localizer, kind: ChangeKindView, effective_at: Timestamp, changes: &[MessageView], tz: &TimeZone) -> String`
  - `pub fn render_report(localizer: &Localizer, report: &ReportView, tz: &TimeZone) -> String`
  - `pub trait Editor { fn edit(&mut self, text: &str) -> io::Result<String>; }` and
    `pub struct TempfileEditor`: writes to a `.toml` temporary file under
    `$XDG_RUNTIME_DIR/selflow/`, runs `sh -c '$VISUAL "$1"'` (else `$EDITOR`, else `vi`) so editor
    commands with arguments work, waits, and reads the file back.
  - `pub enum EditOutcome { Unchanged, Saved(Response), Abandoned }`
  - `pub fn edit_loop(initial: &str, editor: &mut dyn Editor, save: &mut dyn FnMut(&str) -> Response, ask_again: &mut dyn FnMut(&str) -> bool) -> EditOutcome`:
    an unchanged text is never sent; on `Invalid`, `ask_again` receives the rendered errors and the
    editor reopens on the rejected text.

The CLI asks the daemon for `Status` to pick the language. Dates read like "Fri 20 Nov, 10:00" and
"sex 20 nov, 10:00", built from the `weekday-short-*` and `month-short-*` messages.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn renders_errors_with_line_and_column_in_portuguese() {
    let error = PositionedMessage {
        message: MessageView { id: "config-bad-duration".into(), args: vec![("value".into(), "soon".into())] },
        line: 15,
        column: 8,
    };
    let out = render_errors(&Localizer::new(Language::PtBr), &[error]);
    assert!(out.starts_with("linha 15, coluna 8: "));
    assert!(out.contains("soon"));
}

#[test]
fn renders_when_a_loosening_takes_effect() {
    let effective = at(20, 10, 0).timestamp();
    let pt = render_saved(&Localizer::new(Language::PtBr), ChangeKindView::Loosen, effective, &[], &brt());
    let en = render_saved(&Localizer::new(Language::En), ChangeKindView::Loosen, effective, &[], &brt());
    assert!(pt.contains("sex 20 nov, 10:00"));
    assert!(en.contains("Fri 20 Nov, 10:00"));
}

#[test]
fn reopens_the_editor_on_the_rejected_text() {
    let mut editor = ScriptedEditor::new(["bad", "good"]);
    let mut sent = Vec::new();
    let outcome = edit_loop(
        "initial",
        &mut editor,
        &mut |text| {
            sent.push(text.to_owned());
            if text == "bad" { invalid() } else { saved() }
        },
        &mut |_| true,
    );
    assert_eq!(editor.seen(), vec!["initial", "bad"]);
    assert_eq!(sent, vec!["bad", "good"]);
    assert!(matches!(outcome, EditOutcome::Saved(_)));
}

#[test]
fn stops_when_the_person_declines_to_edit_again() {
    let mut editor = ScriptedEditor::new(["bad"]);
    let outcome = edit_loop("initial", &mut editor, &mut |_| invalid(), &mut |_| false);
    assert!(matches!(outcome, EditOutcome::Abandoned));
}

#[test]
fn does_not_send_an_unchanged_file() {
    let mut editor = ScriptedEditor::new(["initial"]);
    let outcome = edit_loop("initial", &mut editor, &mut |_| panic!("nothing should be sent"), &mut |_| true);
    assert!(matches!(outcome, EditOutcome::Unchanged));
}
```

- [ ] **Step 2: Run** `cargo test -p selflow --test render --test edit_loop`. Expected: FAIL.
- [ ] **Step 3: Implement** the CLI, the client, the renderers, the editor and `cli.ftl`.
- [ ] **Step 4: Run** `cargo test -p selflow`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: add the config and report commands`.

### Task 15: Lock screen (`selflow lock`)

**Files:**
- Create: `crates/selflow/src/lock/mod.rs`, `src/lock/model.rs`, `src/lock/view.rs`,
  `src/lock/style.rs`, `src/cli/commands/lock_cmd.rs`
- Create: `locales/en/lock.ftl`, `locales/pt-BR/lock.ftl`
- Modify: `crates/selflow/src/cli/types.rs`, `src/cli/mod.rs` (add the `Lock` command),
  `crates/selflow/tests/support/mod.rs` (add `view`)
- Test: `crates/selflow/tests/lock_model.rs`

**Interfaces:**
- Consumes: `LockView`, `RefusalView`, `Request`, `Response`, `socket_path` (Task 9); `Client`
  (Task 14); `Localizer` (Task 2).
- Produces:
  - `pub struct LockModel` with `new(now: Timestamp) -> Self`,
    `update(&mut self, event: LockEvent) -> LockEffect` and `screen(&self) -> Screen`.
  - `pub enum LockEvent { View(LockView), Ended, Disconnected, Tick(Timestamp), ReasonEdited(String), PhraseEdited(String), Submit, Granted, Refused(RefusalView) }`
  - `pub enum LockEffect { None, SendExtensionRequest { reason: String, phrase: String }, Exit, Reconnect }`
  - `pub enum Screen { Connecting, Locked { label: String, until: Timestamp, shutdown_at: Option<Timestamp>, extension: ExtensionArea } }`
  - `pub enum ExtensionArea { NoneLeft, WaitUntil(Timestamp), Form { minutes: i64, reason_required: bool, phrase: Option<String>, error: Option<RefusalView>, sending: bool } }`
  - `#[command(hide = true)] Lock(LockArgs)` added to `Command`, with
    `pub struct LockArgs { #[arg(long)] pub preview: bool }`.
  - `pub fn lock_cmd(args: LockArgs) -> Result<ExitCode, CliError>`: runs
    `iced_exwlshell::sessionlock::application(..)` with the message enum marked
    `#[to_sessionlock_message]`, following the crate's `counter_lock` example. `LockEffect::Exit`
    returns `iced::exit()`, which releases the session lock. With `--preview`, the same model and
    view run in an ordinary iced window with a fixed sample `LockView` and no daemon connection,
    and Escape closes it. The preview is the only way an agent may look at the lock screen (Agent
    safety rule 1).

**Behavior the tests pin:** `Ended` exits. `Disconnected` reconnects and the screen stays locked,
because a lost daemon must never unlock the screen. Before `extension_available_at` the area waits;
a `Tick` past it shows the form. `Submit` sends one request and ignores further submits until
`Granted` or `Refused` arrives. No extensions left shows `NoneLeft`.

**Wiring.** The daemon connection is an iced subscription: connect to `socket_path()`, send
`SubscribeLock`, map `Lock`, `LockEnded` and end of input to `View`, `Ended` and `Disconnected`,
and reconnect every second. Each extension request uses its own short connection as a `Task`. A
1-second `iced::time::every` feeds `Tick`. The look is calm: a dark neutral background, one muted
accent, the end time in large type, and no animation besides the countdown digits. Enter submits
the form. Before the first view arrives, the screen shows only "Selflow" and stays locked.

- [ ] **Step 1: Write the failing tests** (`view()` is a `LockView` with label `bedtime`, ends at
  07:00, extension available at 23:02, 1 left, 20 minutes, reason required, no phrase, shutdown at
  23:30)

```rust
#[test]
fn shows_the_wait_and_then_the_form() {
    let mut model = LockModel::new(at(19, 23, 0).timestamp());
    model.update(LockEvent::View(view()));
    assert!(matches!(model.screen(), Screen::Locked { extension: ExtensionArea::WaitUntil(t), .. } if t == at(19, 23, 2).timestamp()));
    model.update(LockEvent::Tick(at(19, 23, 2).timestamp()));
    assert!(matches!(model.screen(), Screen::Locked { extension: ExtensionArea::Form { minutes: 20, reason_required: true, .. }, .. }));
}

#[test]
fn sends_one_request_until_an_answer_arrives() {
    let mut model = LockModel::new(at(19, 23, 2).timestamp());
    model.update(LockEvent::View(view()));
    model.update(LockEvent::ReasonEdited("talking to my sister".into()));
    assert_eq!(model.update(LockEvent::Submit), LockEffect::SendExtensionRequest { reason: "talking to my sister".into(), phrase: String::new() });
    assert_eq!(model.update(LockEvent::Submit), LockEffect::None);
    model.update(LockEvent::Refused(RefusalView::ReasonTooShort { min_chars: 10 }));
    assert!(matches!(model.screen(), Screen::Locked { extension: ExtensionArea::Form { error: Some(_), sending: false, .. }, .. }));
}

#[test]
fn the_end_of_the_lock_exits() {
    let mut model = LockModel::new(at(19, 23, 0).timestamp());
    model.update(LockEvent::View(view()));
    assert_eq!(model.update(LockEvent::Ended), LockEffect::Exit);
}

#[test]
fn a_lost_connection_keeps_the_screen_locked() {
    let mut model = LockModel::new(at(19, 23, 0).timestamp());
    model.update(LockEvent::View(view()));
    assert_eq!(model.update(LockEvent::Disconnected), LockEffect::Reconnect);
    assert!(matches!(model.screen(), Screen::Locked { .. }));
}

#[test]
fn shows_no_form_when_the_budget_is_spent() {
    let mut model = LockModel::new(at(19, 23, 30).timestamp());
    model.update(LockEvent::View(LockView { extensions_left: 0, ..view() }));
    assert!(matches!(model.screen(), Screen::Locked { extension: ExtensionArea::NoneLeft, .. }));
}
```

- [ ] **Step 2: Run** `cargo test -p selflow --test lock_model`. Expected: FAIL.
- [ ] **Step 3: Implement** the model, the iced view, the subscription and `lock.ftl`.
- [ ] **Step 4: Run** `cargo test -p selflow`. Expected: PASS.
- [ ] **Step 5: Commit** `chore: add the lock screen`.

### Task 16: Install, Hyprland hook, end-to-end check and measurement

**Files:**
- Create: `contrib/hyprland.lua`
- Modify: `Makefile` (add `install`), `README.md` (install and use)

- [ ] **Step 1: Add a `make install` target** that runs
  `cargo install --locked --path crates/selflowd`, `cargo install --locked --path crates/selflow`,
  `install -Dm644 contrib/selflowd.service ~/.config/systemd/user/selflowd.service` and
  `systemctl --user daemon-reload`. The agent writes the target; the owner runs it (Agent safety
  rule 3).
- [ ] **Step 2: `contrib/hyprland.lua`**, exactly:

```lua
hl.config({ misc = { allow_session_lock_restore = 1 } })

hl.on("hyprland.start", function()
	hl.exec_cmd("systemctl --user restart selflowd.service")
end)
```

- [ ] **Step 3: Show the owner where these lines go, without editing their dotfiles** (they are
  managed with yadm). The `exec_cmd` line goes inside the existing `hyprland.start` handler in
  `~/.config/hypr/lua/services.lua`, after the `dbus-update-activation-environment` line, so the
  service sees `WAYLAND_DISPLAY`. The `hl.config` line goes with their other settings.
- [ ] **Step 4: README.** Install with `make install`, the Hyprland lines, the first
  `selflow config edit` (it opens the default file in the person's language), `selflow config status`
  and `selflow report`. Run the humanizer skill on the prose.
- [ ] **Step 5: End-to-end check, done by the owner.** The agent hands the owner this checklist and
  waits for the results; a failure goes back as a bug into the task that owns the code. In a
  Hyprland session, after `make install` and the Hyprland lines, with `systemctl --user edit selflowd`
  adding `Environment=SELFLOW_POWER=log`:
  1. `selflow config edit`: set `bedtime` 3 minutes ahead and `wake` 15 minutes ahead, remove
     `start_jitter`, set `wait = "1m"` and `shutdown_after = "5m"`. The save says it applies now.
  2. A notification arrives 1 minute before bedtime, and at bedtime every monitor locks.
  3. `pkill -f "selflow lock"`: a new lock screen appears within 2 s and the session never
     unlocks.
  4. With a second monitor available, unplug it and plug it in while locked: it shows the lock.
  5. After 1 minute, write a reason and ask for time: the screen unlocks, a warning comes before
     the lock returns, and the lock returns.
  6. 5 minutes into the returned lock, `journalctl --user -u selflowd` shows `shutdown requested`.
  7. `systemctl --user restart selflowd` during the lock: the lock stays or comes back at once,
     and `selflow report --date <today>` lists each lock start once.
  8. `selflow config edit` moving `bedtime` later: the CLI says it takes effect in 24 hours, and
     `selflow config status` lists the change.
  Then remove the `SELFLOW_POWER` override.
- [ ] **Step 6: Measure the idle daemon** the owner installed, for 10 minutes outside any lock. The
  agent may run these read-only commands:
  `ps -o rss= -p "$(pidof selflowd)"` stays under 30720 (KiB), and the difference in fields 14 and
  15 of `/proc/<pid>/stat` over the 10 minutes, divided by `getconf CLK_TCK`, stays under 1 second.
- [ ] **Step 7: Commit** `feat: lock the screen at bedtime` once the owner reports the checklist
  passing, with the two measurements in the commit body.
