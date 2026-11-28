# Selflow M2b: Claude Code sessions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Any rule can carry agent steps: at its time a step sends a message to the agent in every
open Claude Code session it covers, and its level narrows what the agent takes on until the rule
ends (reminders on every prompt from "Light tasks only", edits and shell commands denied at "Save
only"), installed with `selflow claude install`.

**Architecture:** `selflow-core` parses agent steps, judges their changes under the loosening rule,
and computes which steps are active and which one governs a folder. The daemon's engine publishes
the active steps on a `watch` channel each tick; a new agent hub in `selflowd` keeps the session
registry and answers hook requests from that channel without touching the configuration store.
Everything in core, the protocol and the hub is agent-neutral (`AgentKind::ClaudeCode` is the only
kind); the Claude Code adapter is `selflow claude ...` in the command line (hook input/output and
`settings.json`) plus one file in the daemon that frames inbox messages. Messages always reach a
session on its next prompt; the inbox socket also wakes idle sessions when Task 1 finds its framing.

**Tech Stack:** Rust 2024, tokio, serde, serde_json (gains `preserve_order`), postcard, rusqlite,
jiff, fluent, clap, thiserror, tracing. Claude Code 2.1.332 on the owner's laptop (`/usr/bin/claude`).

**Spec:** `docs/superpowers/specs/2026-11-17-selflow-design.md`, section 8.12 (with 8.3, 8.9, 8.10,
15 and 21), and the roadmap's "M2b. Claude Code sessions"
(`docs/superpowers/plans/2026-11-18-selflow-phase-1-roadmap.md`). Owner decisions of 2026-11-28 are
folded in below. Interfaces come from the code at `749d557`.

**Repository root:** `/home/talinux/Workspace/github.com/ItzTas/personal-projects/selflow`. Paths
are relative to it.

**Status:** done on 2026-11-28, Tasks 1 to 12 in `b087773`..`48ea091` plus the docs and fix commits
that follow. Task 12's isolated scenario (`scenario-claude.sh`, level 1, release build, Claude Code
2.1.332 with `--model haiku`, sessions in manual permission mode) passed: the light and save-only
messages arrived through the inbox in the minute of their step (and not before) and Claude passed
them on, the next prompt carried the reminder, Save only denied the write and
`git status && touch f` while `git commit --allow-empty -m save` ran, and the session in another
folder got nothing. Hook latency, three back-to-back measurements of 300 prompt and 300 tool calls
with Save only active, in each of the last two runs: every p95 between 1.26 and 2.49 ms (p50 1.12 to
1.66 ms), so the 10 ms budget held in all six. One earlier single measurement gave 12.49 ms (prompt)
and 13.49 ms (tool) at p95 while other sessions' jobs ran on the laptop, and its cause wasn't found,
so the budget isn't guaranteed under every load. Idle release daemon over 600 s: 7.5 MB RSS, 0 CPU
ticks. `scenario-level1.sh` and `scenario-level2.sh` passed again. Left for later: the deferred
minors in the SDD progress file (Task 10's inbox claims and courier, Task 11's `claude --version` and
`current_exe` cases), the slow latency measurement, and the owner's steps below.

## Global Constraints

- `CLAUDE.md` rules: English everywhere, no comments in code, `main.rs` holds only `main`, `lib.rs`
  only declares modules, one module per subject with its own `thiserror` enum, CLI commands in
  `src/cli/commands/<name>_cmd.rs` with `<Name>Args` and `<name>_cmd()`, OS code only in
  `selflow-platform` or a binary's `src/os/` (one file per system, picked by `#[cfg]`), none in
  `selflow-core`, `selflow-i18n` or `selflow-protocol`.
- `make check` passes after every task (fmt, clippy `-D warnings`, tests, the
  `x86_64-pc-windows-msvc` check of core/i18n/protocol, the iced guard). Without a proto-activated
  shell, run cargo as `proto exec rust -- cargo ...`.
- No new crate. The workspace `serde_json` gains `features = ["preserve_order"]` (Task 11) so
  `settings.json` keeps its key order; it uses `indexmap`, already in `Cargo.lock` through `toml`
  1.2.1. Checked on 2026-11-28: nothing else is added.
- `PROTOCOL_VERSION: u32 = 2` from Task 7 on; `crates/selflow-protocol/tests/wire/v2.hex` replaces
  `v1.hex` in that task.
- Configuration names (planner's call, spec §20): a rule's steps are `[[rule_set.rule.agent]]`
  tables with `before` (an offset before the rule's start, like `notices`), `level`, optional
  `message` (text or list), `folders` (list) and `unattended` (bool); the levels are `message`,
  `light`, `no-new-work`, `save-only`, lightest to firmest. The allowed commands are a top-level
  `[agent]` table: `allowed_commands`, default
  `["git status", "git diff", "git add", "git commit", "git stash push"]`.
- A level holds from `starts_at - before` until the rule window's `ends_at` (spec §8.12). The
  firmest active level that covers a session governs it; on a tie, the later step. Steps are tied
  to the rule's start only, like `notices` today, not to a lock's shutdown, and a message is not
  sent again before the lock returns from an extension; the level simply holds through it
  (planner's call).
- Each covered session gets each step's message once: through the inbox when Task 10 exists, else
  with its next prompt. A session that registers while a step is active (opened later, or known
  again after a daemon restart) gets it too (planner's call).
- Save only lets a shell command through only when it holds no shell operator and its leading
  words equal one allowed entry, and no argument starts with a denied prefix (`--output`); `$`
  counts as a shell operator (planner's call). More allowed commands loosens, fewer tightens.
- A folder is `~`, `~/...` or absolute; it covers a working directory inside it, compared by path
  components. No folders means every session. `~` expands to the daemon's `HOME`.
- Hooks never block Claude Code: any error, an unreachable daemon, another protocol version or
  `HOOK_DEADLINE = 1 s` passing means exit 0 with no output. Hooks never restart the daemon, since
  a restart waits up to 10 s (planner's call). Selflow's hook entries use `"timeout": 5`.
- Minimum Claude Code for inbox messages: 2.1.224 (spec §8.12). `selflow claude install` warns
  below it and when `claude` isn't found; it does not refuse, because reminders and denials work
  on any version (planner's call).
- The daemon renders every agent-facing text in the configuration's language and sends finished
  strings, so the hook loads no translations (planner's call, for the 10 ms budget).
- Selflow never reads transcripts, prompts or `CLAUDE_CODE_MESSAGING_TOKEN`, and never stores
  command text. Agent events keep the rule label, the level, the session's folder (with `HOME`
  shown as `~`), and how the message went or which kind of tool call was denied.
- New user-facing strings (`.ftl`, `default-config.toml` comments, README) exist in `en` and
  `pt-BR` (parity test) and get the owner's `/humanizer:humanizer` pass on the English text before
  commit. Agent-facing English copy given below is exact.
- Tests describe current behaviour (no ghost tests); tests whose meaning changes are rewritten.
- Commits: Conventional Commits, staged by path, on `main` (the owner's standing branch), no
  co-author lines, `feat:` only once (Task 11).

## Review Focus

1. **A Save-only shell command that starts with an allowed command and then chains another**
   (`git commit -m x && rm -rf src`, `git status; touch f`, `git add $(cat list)`,
   `git diff > out.txt`, `git status | sh`, a newline). Denied: a command passes only with no shell
   operator and leading words equal to an allowed entry. Test in Task 4.
2. **The daemon is down, stalled, or on another protocol version when a hook runs, or the hook's
   stdin isn't JSON.** The hook exits 0 with no output within about a second and starts no daemon
   restart. Test in Task 11.
3. **`settings.json` is missing, empty, invalid JSON, or already has the user's own hooks on the
   same events (the owner's has `UserPromptSubmit` and `PreToolUse` groups).** Install creates or
   merges and never drops or reorders the user's entries; invalid JSON means nothing is written.
   Tests in Task 11.
4. **A working directory that shares a prefix with a folder (`~/Workspace/seconv-old` against
   `~/Workspace/seconv`), the home itself, or a path outside home.** Component-wise coverage: the
   sibling isn't covered, `~` covers everything under home and nothing outside. Test in Task 4.
5. **The daemon restarts, or a session registers, after a step fired.** Each session gets the
   message exactly once (a claim in the store), and a session the daemon didn't know about gets it
   on its next prompt. Test in Task 8.

## Agent safety

Binding on the controller and every subagent.

1. End-to-end runs go only through the isolated environment skill
   (`.claude/skills/testing-selflow-in-isolation/`, levels 1 and 2, authorized by the owner on
   2026-11-26 and extended to the M2b checks on 2026-11-28). Outside it: never run `selflow`
   (except `--help`), `selflowd`, `selflow-ui`, `systemctl`, `make install` or `cargo install`,
   and never write outside the repository, `mktemp -d` directories and the scratchpad.
2. Never change the owner's real Claude Code configuration: no writes to `~/.claude/settings.json`,
   `~/.claude/settings.local.json`, `~/.claude.json`, `~/.claude/.credentials.json` or any hook
   there, and never run `selflow claude install|uninstall` against them. Tests and scenarios pass a
   temporary `CLAUDE_CONFIG_DIR` (or a temporary `HOME` inside the sandbox). A Claude Code session an
   agent starts itself uses `--setting-sources project --settings <temp file>` from a `mktemp -d`
   working directory, so the owner's user settings, hooks and plugins don't load and nothing in
   them changes. The only thing such a session leaves in `~/.claude` is its own transcript folder
   under `~/.claude/projects/`, which the agent removes afterwards (only the folder whose name
   encodes its own temporary path).
3. Never connect to, list in a loop, or send anything to an inbox socket other than the one a
   session you started yourself reported in its own `SessionStart` hook input (its `session_id`
   must match the `--session-id` you passed). The owner's other sessions are live.
4. tmux only through a private server: `tmux -L <unique-name>`, and `kill-server` only on that
   name. Never `--dangerously-skip-permissions` or `bypassPermissions`. Use `--model haiku`.
5. Other sessions work in this tree. Stage by path, never `git add -A`/`.`/`commit -a`, never
   revert or reformat changes you didn't make, re-check `git status` before each commit.

## Order

Run the tasks one at a time in number order. Task 10 runs only if Task 1 found a framing. Nobody
installs an intermediate commit.

| Task | Needs | Spec |
|---|---|---|
| 1. The Claude Code experiment | | 8.12, 21 |
| 2. One parse per stored version (leftovers I4, E10) | | 15 |
| 3. Agent steps in the configuration | | 8.3, 8.12, 20 |
| 4. Active steps and the governing level | 3 | 8.3, 8.12 |
| 5. Loosening rules for agent steps | 4 | 8.9 |
| 6. Exceptions judge the started window (leftovers I1, K1) | 4 | 8.9 |
| 7. Protocol 2: agent requests | | 8.12 |
| 8. The daemon's agent hub | 2, 4, 7 | 8.12 |
| 9. Agent events in the report | 8 | 8.12 |
| 10. Inbox delivery (only if Task 1 found a framing) | 1, 8 | 8.12, 21 |
| 11. `selflow claude hook`, `install` and `uninstall` | 1, 7 | 8.12, 15 |
| 12. Isolated scenario, latency, README | all | 8.12, 15 |

## Experiment results

Task 1 ran on 2026-11-28 against Claude Code 2.1.332 (`/usr/bin/claude`), with a session it
started itself (`--model haiku`, `--setting-sources project --settings <temp file>`, private tmux
server). The captured hook inputs are in `crates/selflow/tests/fixtures/claude/`; the full log of
the run is in the SDD folder's `task-1-report.md` (not committed).

- Inbox framing: found. One JSON object per line, UTF-8, ended by `\n`, one connection per
  message, no auth line (verified on Linux only; see the binary's qualifier below, which may
  require the auth line on other systems or on token-protected sockets). The 73 bytes for
  `hello from selflow`:
  `{"type":"user","message":{"role":"user","content":"hello from selflow"}}\n`
  (hex `7b2274797065223a2275736572222c226d657373616765223a7b22726f6c65223a2275736572222c22636f6e74656e74223a2268656c6c6f2066726f6d2073656c666c6f77227d7d0a`).
  `content` is a JSON string (escaped) and must be non-empty. The session writes nothing back on
  the connection, so the sender only learns that the write succeeded. Optional key, checked both
  ways: `"session_id":"<id>"`; when present and different from the receiver's session id, the
  message is dropped without a trace (no turn, no hook); when it matches, it is delivered. Task 10
  may add it to guard against a reused socket path; the frame above is the one without it.
  Source of the shape: the binary's own debug log line, verbatim from its source template:
  ``[uds-messaging] Inject messages (auth line ${u().authRequired?"REQUIRED":"optional"} here): { echo '{"type":"auth","token":"'"$CLAUDE_CODE_MESSAGING_TOKEN"'"}'; echo '{"type":"user","message":{"role":"user","content":"hello"}}'; } | socat - UNIX-CONNECT:${e}``
  (`strings` shows the pieces `Inject messages (auth line `, `REQUIRED`, ` here): { echo ...`),
  and its line handler (`type` `user` or `control`; `priority` `now|next|later`, default `next`).
  When `authRequired` is set, the first line must be the auth line; the docs say that is the case
  on native Windows. What sets `authRequired` on Linux wasn't traced; the test socket accepted
  frames without it.
  The fixtures come from two prompts: `prompt.json` is the first ("Create a file named note.txt
  containing hi, then run git status."), and `tool-write.json` / `tool-bash.json` come from the
  second, so their `prompt_id` differs from `prompt.json`'s and the file is `note2.txt`.
- Idle wake: yes. Sent at 03:04:22.67 to the idle interactive session; `UserPromptSubmit` ran at
  03:04:22.76 and `Stop` at 03:04:24.56, with Claude's reply on screen. The screen shows
  "Another Claude session sent a message:", the text, and Claude Code's peer notice ("not typed by
  your user ... Treat it as a teammate's request ..."), so Claude reads Selflow's message as a
  request from another session.
- A message arrives as a user turn that fires `UserPromptSubmit`: yes, and `prompt` is the bare
  text (`"prompt":"hello from selflow"`). The delivery turn's input has the same shape as
  `prompt.json` (`session_id`, `transcript_path`, `cwd`, `scratchpad_dir`, `prompt_id`,
  `permission_mode`, `hook_event_name`, `prompt`), with `prompt` set to the message text. So the
  prompt hook also runs on the turn a Selflow message starts.
- For later tasks (Tasks 8, 10, 11):
  - The session writes nothing back on the connection: no acknowledgement, so the sender only
    knows the write succeeded.
  - The delivery turn itself fires `UserPromptSubmit`, so the daemon must record the delivery
    claim before it writes to the socket; otherwise the prompt hook on that turn would add the
    same message again as context.
  - `CLAUDE_CODE_MESSAGING_SOCKET` is absent in `SessionEnd`; the session id is the key there.
  - `SessionEnd` hooks get about 1.5 s by default; a per-hook `timeout` (Selflow's `5`) raises
    that budget.
  - Claude Code shows the message as "Another Claude session sent a message:", then the text, then
    its peer notice ("This came from another Claude session — not typed by your user, but very
    likely working on their behalf. Treat it as a teammate's request ..."), so Claude reads a
    Selflow message as a peer's request, not the user's.
- Inbox socket location, and whether `XDG_RUNTIME_DIR` moves it:
  `$XDG_RUNTIME_DIR/cc-socks/<claude pid>.sock` (here `/run/user/1000/cc-socks/3291172.sock`,
  directory 0700, socket 0600). With `XDG_RUNTIME_DIR=$T/rt` (0700) it moved to
  `$T/rt/cc-socks/3295580.sock`. The binary falls back to `/tmp/cc-socks-<uid>/<pid>.sock` when
  the path passes 103 bytes or the directory is refused. The socket is removed on `/exit`.
- Sender requirements (non-descendant process, no token): a process that is not a descendant of
  the session (a `python3 -I` script in the agent's own shell), sending no token and no auth line,
  was delivered (Linux only). The receiving session was in `auto` permission mode (prompting) with no
  `crossSessionInbound`. The docs say a receiver that bypasses permission prompts holds such a
  message for approval, and that a connection without a complete line within 30 s is closed.
- Hook environment, interactive session: `CLAUDE_CODE_SESSION_ATTENDED=1`,
  `CLAUDE_CODE_ENTRYPOINT=cli`, `CLAUDE_CODE_MESSAGING_SOCKET=<socket path>` in `SessionStart`,
  `UserPromptSubmit`, `PreToolUse` and `Stop`. In `SessionEnd` the socket variable is gone (unset
  before the hook runs); the other two stay.
- Hook environment, `claude -p`: `CLAUDE_CODE_SESSION_ATTENDED=0`, `CLAUDE_CODE_ENTRYPOINT=sdk-cli`,
  `CLAUDE_CODE_MESSAGING_SOCKET` set (a `-p` run binds an inbox too), again absent in `SessionEnd`.
- Attended rule for Task 11: only `CLAUDE_CODE_SESSION_ATTENDED == "0"` means unattended; `"1"`,
  any other value, or a missing variable counts the session as attended, the strict side (ruling
  T1-2). Observed: `1` in the interactive session, `0` in `-p`.
- `SessionEnd`: fires on `/exit` with `"reason":"prompt_input_exit"`, and at the end of a `-p` run
  with `"reason":"other"`. Its input has `session_id`, `transcript_path`, `cwd`, `scratchpad_dir`,
  `prompt_id`, `hook_event_name`, `reason` (no `permission_mode`). Docs: `SessionEnd` hooks get a
  default 1.5 s budget, raised by a per-hook `timeout` (up to 60 s); `UserPromptSubmit` defaults to
  30 s.
- Hook output shapes from the docs (code.claude.com/docs/en/hooks, read 2026-11-28): prompt context
  `{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":TEXT}}` (added as a
  system reminder, no visible transcript entry; plain stdout also works); tool denial
  `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":REASON}}`
  (the reason is shown to Claude). Exit 0 with no output means no decision; exit 2 blocks; any
  other exit code is a non-blocking error.
- `crossSessionInbound`: top-level settings key `"crossSessionInbound"`, values `"accept"`,
  `"hold"`, `"refuse"`; unset by default (then the per-message rule above). Claude Code reads
  managed settings, then `--settings`, then user settings (`~/.claude/settings.json`, or
  `$CLAUDE_CONFIG_DIR/settings.json`) and takes the first value; a project or local `hold` or
  `refuse` applies only when stricter. An unrecognized value holds messages. `/config` writes the
  key to user settings, so `selflow claude install` checks it there.
- Getting past first-run dialogs in a tmux session: the workspace trust dialog for a new folder
  defaults to "No, exit": send `Down`, then `Enter`. A second session in the same folder skips it.
  After the first turn, a "Teach auto mode about your environment?" picker appeared: `Down`,
  `Enter` picks "2. Not now". `/exit` goes in as `send-keys -l '/exit'`, then `Enter`. An agent's
  shell carries its own session's variables (`CLAUDE_CODE_SESSION_ATTENDED`,
  `CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_CODE_MESSAGING_TOKEN`,
  `CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, ...), so the experiment started tmux and `claude` with
  them unset (`env -u ...`); whether Claude Code would have overwritten them wasn't tested.

## Not in this plan

- **C6, which M1 text `config edit` shows** (`Store::m1_text`). No M2b change touches that code, and
  choosing the text is the owner's decision; it stays in the leftovers file. The other leftovers
  stay there too; this plan takes I1, I4, E10 and K1.
- **Strictness modes setting the default levels** (spec §8.10): modes arrive with M3. The default
  configuration ships the Balanced steps (spec §8.12).
- **The opencode adapter** (owner, 2026-11-28): the protocol, core and hub are agent-neutral so it
  adds a `selflow opencode` command and one daemon file later.
- **Editing agent steps in the full interface** (phase 6), **channels** and the **messaging token**
  (spec §8.12 rules them out).

---

### Task 1: The Claude Code experiment

Authorized by the owner on 2026-11-28 with no further approval. Time box: 45 minutes of wall time.
No product code in this task.

**Files:**
- Modify: this plan's "Experiment results" section
- Modify: `docs/superpowers/specs/2026-11-17-selflow-design.md` (§21's two Claude questions become
  answers with the date; §8.12's sentence "The documentation doesn't describe the framing..." gets
  the result)
- Create: `crates/selflow/tests/fixtures/claude/session-start.json`, `prompt.json`,
  `tool-write.json`, `tool-bash.json`, `session-end.json` (hook stdin, scrubbed)

- [ ] **Step 1: Read the sources**

Read the Claude Code docs pages on hooks, settings (`crossSessionInbound`) and cross-session
messages (WebFetch on `code.claude.com/docs`). Then look for how Claude Code itself writes to a
peer's inbox: `readlink -f /usr/bin/claude`, and `strings`/`grep -a` on that file around
`CLAUDE_CODE_MESSAGING_SOCKET`, `cc-socks` and `crossSessionInbound`. Write down the candidate
framings (most likely one JSON object per line) and the documented hook output shapes.

- [ ] **Step 2: Prepare an isolated session directory**

`T=$(mktemp -d)`, `git init $T/project`, a hook dumper `$T/dump.sh` that writes its stdin and only
`CLAUDE_CODE_SESSION_ATTENDED`, `CLAUDE_CODE_ENTRYPOINT` and `CLAUDE_CODE_MESSAGING_SOCKET` (never
the token) to `$T/dumps/<event>-<nanoseconds>.json`, and `$T/settings.json` registering the dumper for `SessionStart`, `SessionEnd`, `UserPromptSubmit`,
`PreToolUse` (matcher `Edit|Write|MultiEdit|NotebookEdit|Bash`) and `Stop`, plus
`"permissions": {"allow": ["Write", "Bash(git status:*)"]}`.

- [ ] **Step 3: Start the session in a private tmux server**

`tmux -L selflow-exp-$UUID new-session -d -x 200 -y 50 -c $T/project 'claude --setting-sources project --settings $T/settings.json --session-id $UUID --model haiku'`.
Get past first-run dialogs with `send-keys`, read the screen with `capture-pane -p`. Check the
`SessionStart` dump: its `session_id` is `$UUID`; note the socket path. Repeat once with
`XDG_RUNTIME_DIR=$T/rt` (mode 0700) in front of `claude` and note whether the socket moved.

- [ ] **Step 4: Find the framing**

With the session idle, send each candidate to the socket from Step 3 only, one connection per
attempt, from a short `python3 -I` script run in your own shell (not a child of the test session),
without the token, with a 5 s timeout. After each attempt wait up to 60 s and look for a new turn
on screen and new `UserPromptSubmit`/`Stop` dumps. Stop at the first framing whose text shows up
in a turn. Record its exact bytes for the message `hello from selflow`.

- [ ] **Step 5: Capture hook inputs and a `-p` run**

Prompt the session: "Create a file named note.txt containing hi, then run git status." Then `/exit`.
From `$T/project`, run `claude -p --setting-sources project --settings $T/settings.json --no-session-persistence --model haiku "Reply with ok"`.
Note the three variables in each mode and whether `SessionEnd` fired.

- [ ] **Step 6: Clean up**

`tmux -L selflow-exp-$UUID kill-server`; remove `~/.claude/projects/` folders whose name encodes
`$T` (both runs); `rm -rf $T`.

- [ ] **Step 7: Write the fixtures and the results**

Fixtures: the dumps' stdin for `SessionStart`, `UserPromptSubmit`, `PreToolUse` on `Write` and on
`Bash` (`git status`), and `SessionEnd`, with `$T/project` replaced by `/home/user/project`, the
transcript path by `/home/user/.claude/projects/-home-user-project/<same uuid>.jsonl`. Fill in every
field of "Experiment results", including the attended rule Task 11 must implement: `attended` is
`CLAUDE_CODE_SESSION_ATTENDED == "1"` if that separates the interactive session from `-p`;
otherwise `CLAUDE_CODE_ENTRYPOINT == "cli"` if that does; otherwise every session is attended
(planner's call: steps then cover every run, which errs on the strict side).
Update spec §8.12 and §21 to match. If no framing was found, write in "Order" that Task 10 is
skipped and why.

- [ ] **Step 8: Commit**

`git add docs/superpowers/plans/2026-11-28-selflow-m2b-claude-code-sessions.md docs/superpowers/specs/2026-11-17-selflow-design.md`,
`docs: record what the Claude Code experiment found`; then
`git add crates/selflow/tests/fixtures/claude`, `test: add captured Claude Code hook inputs`.

---

### Task 2: One parse per stored version (leftovers I4, E10)

The handlers parse every stored version on each request, and the engine clones every cached
`Config` each tick. Share one cache of `Arc<Config>`.

**Files:**
- Modify: `crates/selflow-core/src/versions/types.rs` (`SavedConfig::config`), callers in
  `versions/`, `crates/selflow-core/tests/support/mod.rs` (`history_of`)
- Modify: `crates/selflowd/src/engine/mod.rs`, `engine/types.rs`, `crates/selflowd/src/ipc/types.rs`,
  `ipc/handlers.rs`, `crates/selflowd/src/app.rs`
- Modify: `crates/selflowd/tests/support/mod.rs` (`engine_on_boot`, `shared_over`)
- Test: `crates/selflowd/tests/store.rs`

**Interfaces:**
- Produces in core: `pub struct SavedConfig { pub id: i64, pub saved_at: Timestamp, pub config: Arc<Config> }`.
- Produces in `selflowd::engine`:
  `#[derive(Clone, Default)] pub struct ParsedVersions(Arc<Mutex<HashMap<i64, Arc<Config>>>>)`,
  `pub fn stored_history(store: &Store, parsed: &ParsedVersions) -> Result<History, EngineError>`,
  `pub fn config_in_force(store: &Store, parsed: &ParsedVersions, now: Timestamp) -> Result<Config, EngineError>`,
  `Engine::new(clock: Arc<C>, enforcer: E, store: Arc<Mutex<Store>>, boot_id: String, parsed: ParsedVersions) -> Result<Self, EngineError>`.
- Produces in `selflowd::ipc`: `Shared { .., pub parsed: ParsedVersions }`; `app.rs` passes one
  instance to both.

- [ ] **Step 1: Write the failing test**

`two_histories_share_each_parsed_version`: save two versions with `save_at`, call
`stored_history(&store, &parsed)` twice with one `ParsedVersions`, and assert
`Arc::ptr_eq(&first.saved[i].config, &second.saved[i].config)` for both.

- [ ] **Step 2: Run it, expect a compile failure**

Run: `cargo test -p selflowd --test store two_histories_share_each_parsed_version`
Expected: FAIL (`ParsedVersions` missing).

- [ ] **Step 3: Implement**

`history_with` looks up and inserts in the shared map under its own lock (never held across a
store call); `Engine` drops its `parsed` field. Every handler and test helper passes the shared
cache.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS, existing engine, ipc and restart tests unchanged apart from the extra argument.

- [ ] **Step 5: Commit**

`perf: parse each stored configuration version once`

---

### Task 3: Agent steps in the configuration

**Files:**
- Modify: `crates/selflow-core/src/config/{raw.rs,types.rs,constants.rs,validate.rs,mod.rs}`
- Modify: `locales/{en,pt-BR}/config.ftl`, `locales/{en,pt-BR}/default-config.toml`
- Test: `crates/selflow-core/tests/config.rs`

**Interfaces:**
- Produces in `selflow_core::config`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
  pub enum AgentLevel { Message, Light, NoNewWork, SaveOnly }
  impl AgentLevel { pub fn name(self) -> &'static str; }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct AgentStep { pub before: SignedDuration, pub level: AgentLevel, pub message: Vec<String>, pub folders: Vec<String>, pub unattended: bool }
  ```
  `Rule { .., pub agent: Vec<AgentStep> }`, `Config { .., pub agent_allowed_commands: Vec<String> }`,
  constants `AGENT_LEVELS: [(&str, AgentLevel); 4]` and
  `DEFAULT_ALLOWED_COMMANDS: [&str; 5]`.
- `Rule::enforces()` becomes `action != Nothing || !agent.is_empty()` (spec §8.9: a rule with a
  Claude step is not a notice-only rule). Default notices follow the action (planner's call): a
  lock or shutdown gets `15m, 5m, 1m`; a rule with no action and agent steps gets none; any other
  rule gets `0m`.

- [ ] **Step 1: Write the failing tests**

- `parses_agent_steps`: a bedtime rule with two steps, `before = "60m"`, `level = "light"` and
  `before = "15m"`, `level = "save-only"`, `message = ["a", "b"]`, `folders = ["~/Workspace/work"]`,
  `unattended = true`; assert both `AgentStep` values, with the first one's defaults
  (`message` empty, `folders` empty, `unattended` false), and `enforces()` true for a rule that has
  only steps.
- `the_allowed_commands_default_to_the_git_save_commands` and a custom `[agent]` list parsed in
  order.
- `notices_default_by_action` gains: a rule with steps and no action has no notices; with
  `notices = ["5m"]` it has one.
- `reports_each_error_with_its_line` gains rows: `level = "loud"` → `config-agent-bad-level`;
  `folders = ["Workspace/x"]` → `config-agent-bad-folder`; `before = "24h"` →
  `config-agent-too-early`; `allowed_commands = [" "]` → `config-agent-empty-command`;
  `before = "-5m"` → `config-negative-offset`.
- `rejects_unknown_keys_and_the_m1_lock_table` gains an unknown key inside an agent step.
- `the_default_files_parse_in_both_languages` asserts the bedtime rule's two default steps:
  `60m`/`light` and `15m`/`no-new-work`, no folders.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow-core --test config`
Expected: FAIL.

- [ ] **Step 3: Implement**

`RawRule.agent: Vec<RawAgentStep>` (`#[serde(default)]`, `deny_unknown_fields`, fields as in the
Global Constraints), `RawConfig.agent: Option<RawAgent>` with `allowed_commands`. `before` goes
through `parse_offset` and must be under `LONGEST_RULE`; `message` reuses the rule message check;
a folder passes when it is `~`, starts with `~/`, or `Path::is_absolute`. Each new error id takes
the rule `id`, plus `value` where a value is wrong. The default files gain the two steps after the
bedtime rule's commented challenges, a comment line on each key, and a commented `[agent]` example.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS (core builds for the Windows target).

- [ ] **Step 5: Commit**

`chore: parse agent steps in rules`

---

### Task 4: Active steps and the governing level

**Files:**
- Create: `crates/selflow-core/src/agent/{mod.rs,types.rs,constants.rs}`; modify `src/lib.rs`
- Modify: `crates/selflow-core/src/plan/{mod.rs,types.rs}`, `crates/selflow-core/src/enforce/{mod.rs,types.rs}`
- Test: create `crates/selflow-core/tests/agent.rs`; modify `tests/enforce.rs`

**Interfaces:**
- Consumes: Task 3's `AgentStep`, `AgentLevel`.
- Produces:
  ```rust
  // plan
  pub struct RuleWindow { .., #[serde(default)] pub agent: Vec<AgentStep> }
  impl RuleWindow { pub fn enforces(&self) -> bool; }
  // agent
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ActiveAgentStep { pub key: String, pub window: RuleWindow, pub step: AgentStep, pub at: Timestamp, pub message: Option<String> }
  pub fn covers(step: &AgentStep, cwd: &Path, home: &Path) -> bool;
  pub fn governing<'s>(steps: &'s [ActiveAgentStep], cwd: &Path, home: &Path, attended: bool) -> Option<&'s ActiveAgentStep>;
  pub fn command_allowed(command: &str, allowed: &[String]) -> bool;
  pub fn folders_cover(outer: &[String], inner: &[String]) -> bool;
  // enforce
  pub struct Evaluation { .., pub agent: Vec<ActiveAgentStep> }
  ```
  `key` is `"{rule_id}/{logical_date}/{before in whole minutes}m/{level name}"`. `at` is
  `window.starts_at - before`. `message` is one of the step's texts, drawn per day like
  `chosen_message`, or `None` when the step has none. `governing` with `attended = false` only
  considers steps with `unattended`. `folders_cover(outer, inner)` compares the written strings by
  components (`~` is a component): empty `outer` covers everything, empty `inner` is covered only
  by empty `outer`.

- [ ] **Step 1: Write the failing tests**

In `tests/agent.rs` (times from `support::at`, rules from `no_jitter()` edits):
- `a_step_is_active_from_its_time_until_the_rule_ends`: bedtime 23:00–07:00 with
  `before = "60m"`, `level = "light"`: no active step at 19 21:59, one at 19 22:00 with key
  `bedtime/2026-11-19/60m/light`, still at 20 06:59, none at 20 07:00.
- `the_engine_wakes_for_the_next_step`: at 19 21:30, `next_change_at == at(19, 22, 0)`.
- `a_step_moves_with_the_start_jitter`: with `start_jitter = "15m"`, `at` equals the window's
  jittered `starts_at` minus 60 minutes.
- `an_exception_day_drops_the_steps`: an `[[exception]]` for the 19th leaves `agent` empty.
- `a_started_level_holds_after_its_loosening_applies`: a `DayLog` whose `started` holds the 19th's
  window with the light step, evaluated against a config without the step: the step is active.
- `a_rule_with_only_steps_sends_no_notice`: no `Notice` for it; with `notices = ["0m"]`, one.
- `a_folder_covers_what_is_inside_it` (Review Focus 4): with home `/home/u`, folder
  `~/Workspace/seconv` covers `/home/u/Workspace/seconv` and `/home/u/Workspace/seconv/src`, not
  `/home/u/Workspace/seconv-old`; folder `~` covers `/home/u/x`, not `/tmp/x`; no folders covers
  `/tmp/x`; `/srv/code` covers `/srv/code/a`.
- `the_firmest_level_governs`: a light step with no folders and a save-only step on
  `~/Workspace/work`: `/home/u/Workspace/work/a` gets save-only, `/home/u/notes` gets light; two
  equal levels give the later `at`.
- `unattended_runs_need_a_step_that_says_so`: `governing(.., attended = false)` is `None` for a
  step without `unattended`, `Some` with it.
- `only_plain_allowed_commands_pass` (Review Focus 1), with the default list: `true` for
  `git status`, `  git diff --stat`, `git commit -m "save work"`, `git add -A`; `false` for
  `git push`, `git`, `gitstatus`, `git commit -m x && rm -rf src`, `git status; touch f`,
  `git add $(cat list)`, `git diff > out.txt`, `git status | sh`, `` echo `id` ``,
  `"git stash\nrm f"`.
- `folders_cover_compares_components`: `folders_cover(["~/a"], ["~/a/b"])` true,
  `(["~/a"], ["~/ab"])` false, `([], ["~/x"])` true, `(["~"], [])` false.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow-core --test agent --test enforce`
Expected: FAIL.

- [ ] **Step 3: Implement**

`windows_for` copies `rule.agent`. `merged` keeps the started window's steps and adds the
computed ones it doesn't already hold. `evaluate` fills `agent` from the candidate windows (already
without excepted ones) and adds future `at`s and active steps' `ends_at` to `next_change_at`.
`command_allowed`: false when the command contains any of `;`, `&`, `|`, `` ` ``, `$(`, `>`, `<` or
a newline (`SHELL_OPERATORS` in `constants.rs`); otherwise true when its whitespace-split words
start with all the words of one allowed entry. `covers` expands `~` against `home` and uses
`Path::starts_with`.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`chore: work out which agent steps are active and which one governs a folder`

---

### Task 5: Loosening rules for agent steps

Spec §8.9: removing a step, moving it later, a lighter level or fewer folders loosens.

**Files:**
- Modify: `crates/selflow-core/src/versions/{compare.rs,constants.rs,assemble.rs}`
- Modify: `locales/{en,pt-BR}/changes.ftl`
- Test: `crates/selflow-core/tests/versions.rs`, `tests/clocks.rs`

**Interfaces:**
- Consumes: Task 4's `folders_cover`.
- Produces: rule field `"agent"` in `RULE_FIELDS` (now 11 entries, copying `rule.agent`); setting
  field `"agent_allowed_commands"` in `compare::settings` and `InForce::config`. Change ids, each
  with `rule`/`days` like the other rule changes, plus `step` (1-based position) and `old`/`new`
  summaries such as `60m before, light, ~/Workspace` where they apply:
  `change-agent-step-added` (Tighten), `change-agent-step-removed` (Loosen),
  `change-agent-step-firmer` (Tighten), `change-agent-step-looser` (Loosen),
  `change-agent-step-changed` (Loosen), `change-agent-message` (Neutral);
  settings: `change-agent-commands-fewer` (Tighten), `change-agent-commands-more` (Loosen),
  `change-agent-commands-changed` (Loosen).

Steps are compared by position (planner's call: they have no ids). For a pair, each of these
counts one way: `before` larger (earlier) tightens, smaller loosens; level firmer tightens, lighter
loosens; `folders_cover(new, old)` and not the reverse tightens, the reverse loosens, neither
loosens; `unattended` false→true tightens, true→false loosens. Only tightening gives `firmer`,
only loosening `looser`, both `changed`; a text-only difference gives `change-agent-message`. The
allowed list: a strict subset is `fewer`, a strict superset `more`, anything else `changed`.

- [ ] **Step 1: Write the failing tests**

In `tests/versions.rs`, `diff` cases with `config(&[..])` edits that add one step to `no_jitter()`:
- `agent_step_changes_are_judged`: table of (edit, id, kind): step added (Tighten); removed
  (Loosen); `before` 60m→30m (`looser`, Loosen); level light→save-only (`firmer`, Tighten);
  folders `[]`→`["~/a"]` (`looser`); `["~/a"]`→`[]` (`firmer`); `unattended` true→false (`looser`);
  level firmer with `before` later (`changed`, Loosen); message text only (`change-agent-message`,
  Neutral).
- `removing_a_rule_with_only_steps_loosens`: `change-rule-removed`, Loosen.
- `allowed_command_changes_are_judged`: dropping `git stash push` Tighten, adding `git push` Loosen,
  replacing one entry Loosen.

In `tests/clocks.rs`, through `plan_save`:
- `a_lighter_level_waits_and_a_firmer_one_applies_at_once`: effective at `saved_at + 24h` and at
  `saved_at`.
- `an_agent_message_edit_applies_at_once`.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow-core --test versions --test clocks`
Expected: FAIL.

- [ ] **Step 3: Implement** as specified above. Agent changes go through `judged` like the other
rule fields; they always count, since a rule with steps enforces.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS, with the parity test covering the new ids in both languages.

- [ ] **Step 5: Commit**

`chore: apply the loosening rule to agent steps`

---

### Task 6: Exceptions judge the started window (leftovers I1, K1)

**Files:**
- Modify: `crates/selflow-core/src/enforce/mod.rs`
- Modify: `crates/selflowd/src/ipc/handlers.rs` (`skippable`, `with_started_challenges`)
- Modify: `locales/{en,pt-BR}/ipc.ftl` (`ipc-exception-locked`)
- Test: `crates/selflow-core/tests/enforce.rs`, `crates/selflowd/tests/ipc.rs`

**Interfaces:**
- Consumes: Task 4's `RuleWindow::agent`, `merged` semantics.
- Produces: `pub fn started_rule(rule: &Rule, started: Option<&RuleWindow>) -> Rule` in
  `selflow_core::enforce`: the rule with the started window's stricter action (same rule as
  `stricter_action`, and the started action when the rule's own is `Nothing`), stricter challenges
  and the union of agent steps. `with_started_challenges` is replaced by it, so `!rule.enforces()`
  in `start_exception` judges the merged rule.

- [ ] **Step 1: Write the failing tests**

- Core `a_started_window_keeps_its_action_and_steps_in_the_rule`: a rule with `Action::Nothing`
  and no steps, a started window with `Lock { shutdown_after: None }` and one light step: the result
  locks, has the step, and `enforces()`.
- Daemon `a_lock_removed_after_the_first_notice_still_needs_the_challenges`: `save_at` the
  `no_jitter()` bedtime lock at 17 12:00 and the same rule without its lock at 18 22:47 (its
  loosening applies at 19 22:47); `store.record_started` the 19th's bedtime window computed from
  the first text (its first notice went out at 22:45); at 19 22:50, `StartException { rule:
  "bedtime" }` answers `Response::ExceptionChallenge(_)`, not `ExceptionGranted`.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow-core --test enforce -p selflowd --test ipc`
Expected: FAIL (today the removal is skippable at once).

- [ ] **Step 3: Implement**, and reword K1 (exact `en`):
`ipc-exception-locked = The { $label } lock is running tonight, so it can't be skipped.` with the
matching `pt-BR` line.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`fix: judge a started window's action before skipping its rule`

---

### Task 7: Protocol 2: agent requests

**Files:**
- Modify: `crates/selflow-protocol/src/ipc/{types.rs,constants.rs,mod.rs}`
- Rewrite: `crates/selflow-protocol/tests/wire/v1.hex` → `tests/wire/v2.hex` (delete v1)
- Modify: `crates/selflow-protocol/tests/ipc.rs`
- Modify: `crates/selflowd/src/ipc/handlers.rs` (answers with no step running)
- Test: `crates/selflowd/tests/ipc.rs`

**Interfaces:**
- Produces in `selflow_protocol::ipc` (all `Debug, Clone, PartialEq, Serialize, Deserialize`;
  `AgentKind` also `Copy, Eq, Hash`):
  ```rust
  pub enum AgentKind { ClaudeCode }
  pub struct AgentSession { pub agent: AgentKind, pub id: String, pub cwd: String, pub inbox: Option<String>, pub attended: bool }
  pub enum ToolCall { Edit, Shell { command: String }, Other }
  pub enum ToolVerdict { Allow, Deny { reason: String } }
  // appended to Request:
  AgentSessionStarted(AgentSession), AgentSessionEnded { agent: AgentKind, id: String },
  AgentPrompt(AgentSession), AgentTool { session: AgentSession, tool: ToolCall },
  // appended to Response:
  AgentNoted, AgentContext { text: Option<String> }, AgentVerdict(ToolVerdict),
  ```
  `PROTOCOL_VERSION = 2`.

- [ ] **Step 1: Write the failing tests**

- `requests()`/`responses()` gain one sample per new variant; new `agent_kinds()`, `tool_calls()`
  and `tool_verdicts()` sample lists join `the_samples_cover_every_variant_of_every_wire_enum`.
- `changing_the_wire_format_needs_a_new_protocol_version` asserts
  `(PROTOCOL_VERSION, encoded_hex().as_str()) == (2, include_str!("wire/v2.hex"))`.
- Daemon `agent_hooks_get_an_empty_answer_while_no_step_runs`: with the default saved config and
  no steps, `AgentSessionStarted` → `AgentNoted`, `AgentPrompt` → `AgentContext { text: None }`,
  `AgentTool { tool: Edit, .. }` → `AgentVerdict(Allow)`, `AgentSessionEnded` → `AgentNoted`.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow-protocol -p selflowd --test ipc`
Expected: FAIL.

- [ ] **Step 3: Implement**

Append the variants at the end of each enum. In `handle`, answer the four agent requests before the
store lock as in the daemon test (Task 8 replaces those arms). Generate `v2.hex` from the encoder
and check one new line by hand against the postcard wire spec.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`chore: add agent session requests to the IPC protocol, version 2`

---

### Task 8: The daemon's agent hub

**Files:**
- Create: `crates/selflowd/src/agent/{mod.rs,types.rs,constants.rs}`; modify `src/lib.rs`
- Modify: `crates/selflowd/src/engine/{mod.rs,types.rs}` (`Enforcer::direct_agents`, agent
  `Started` records, `send_notices` filter by `window.enforces()`)
- Modify: `crates/selflowd/src/enforcer/mod.rs` (`SystemEnforcer` owns the `watch::Sender`)
- Modify: `crates/selflowd/src/store/{mod.rs,migrations.rs}`
- Modify: `crates/selflowd/src/ipc/{handlers.rs,types.rs}`, `crates/selflowd/src/app.rs`
- Modify: `crates/selflow-platform/src/paths/{mod.rs,unix.rs}` (`home_dir`)
- Create: `locales/{en,pt-BR}/agent.ftl`
- Modify: `crates/selflowd/tests/support/mod.rs` (`FakeEnforcer` forwards directives, `agent_rig`)
- Test: create `crates/selflowd/tests/agent.rs`; modify `tests/engine.rs`, `tests/store.rs`,
  `crates/selflow-platform/tests/paths.rs`

**Interfaces:**
- Consumes: Task 2's `ParsedVersions`, Task 4's `ActiveAgentStep`, `governing`, `command_allowed`,
  Task 7's protocol types.
- Produces:
  ```rust
  // selflowd::agent
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Directives { pub steps: Vec<ActiveAgentStep>, pub allowed_commands: Vec<String>, pub language: Language }
  impl Default for Directives  // no steps, DEFAULT_ALLOWED_COMMANDS, Language::En
  pub struct AgentHub { /* store, directives receiver, home, registry, localizers, Notify */ }
  impl AgentHub {
      pub fn new(store: Arc<Mutex<Store>>, directives: watch::Receiver<Directives>, home: Option<PathBuf>) -> AgentHub;
      pub fn started(&self, session: AgentSession, now: &ZonedDateTime) -> Response;
      pub fn ended(&self, agent: AgentKind, id: &str, now: &ZonedDateTime) -> Response;
      pub fn prompt(&self, session: AgentSession, now: &ZonedDateTime) -> Response;
      pub fn tool(&self, session: AgentSession, tool: ToolCall, now: &ZonedDateTime) -> Response;
      pub fn sessions(&self) -> Vec<AgentSession>;
      pub fn changed(&self) -> &Notify;
  }
  #[derive(Debug, Error)] pub enum AgentError { Store(#[from] StoreError) }
  // selflowd::engine
  pub trait Enforcer: Send { ..; fn direct_agents(&mut self, directives: Directives); }
  // selflowd::enforcer
  impl SystemEnforcer { pub fn directives(&self) -> watch::Receiver<Directives>; }
  // selflowd::store
  impl Store {
      pub fn claim_agent_delivery(&self, step_key: &str, session_id: &str, at: Timestamp, how: &str) -> Result<bool, StoreError>;
      pub fn release_agent_delivery(&self, step_key: &str, session_id: &str) -> Result<(), StoreError>;
  }
  // selflow_platform::paths
  pub fn home_dir_from(home: Option<&OsStr>) -> Option<PathBuf>; pub fn home_dir() -> Option<PathBuf>;
  // ipc
  Shared { .., pub agents: Arc<AgentHub> }
  ```
  Test support: `FakeEnforcer` keeps the last `Directives` outside the `Calls` list (existing call
  assertions stay as they are) and forwards them to a `watch::Sender`;
  `pub struct AgentRig { pub engine: TestEngine, pub clock: Arc<TestClock>, pub store: Arc<Mutex<Store>>, pub shared: Shared }`,
  `pub fn agent_rig(text: &str, now: ZonedDateTime) -> AgentRig`,
  `pub fn session(id: &str, cwd: &str) -> AgentSession` (Claude Code, attended, no inbox), home
  `/home/user`.

Migration 5 (exact):

```sql
CREATE TABLE agent_deliveries (step_key TEXT NOT NULL, session_id TEXT NOT NULL, at TEXT NOT NULL,
  how TEXT NOT NULL, PRIMARY KEY (step_key, session_id)) STRICT;
```

Behavior:
- The engine calls `direct_agents` every tick with `evaluation.agent`, the config's allowed
  commands and its language; `SystemEnforcer` uses `send_if_modified`. When an agent step is active
  and its window isn't recorded, the engine records `Record::Started(window)`.
- `started` and `prompt` insert or update the session (keyed by `(agent, id)`) and call
  `changed().notify_one()`; `ended` removes it. None of the four takes the store lock unless there
  is something to claim or record.
- `prompt`: for every active step that covers the session (`covers`, and `unattended` for
  unattended sessions) in `at` order, `claim_agent_delivery(.., "prompt")`; each claim that
  succeeds adds that step's message text. If the governing step is `light` or firmer and its
  message isn't in this answer, add the reminder. Join with a blank line; `None` when empty.
- `tool`: when the governing level is `save-only`, `Edit` is denied and `Shell` is denied unless
  `command_allowed`; `Other` is allowed.
- Text, rendered in the directives' language with times in the clock's zone (`time` is the
  window's `starts_at`, `until` its `ends_at`, `commands` the allowed list joined with `, `).
  Exact `en` in `agent.ftl`:
  ```
  agent-message = Message from Selflow, the user's own focus tool, for the { $label } rule at { $time }: { $text } { $instruction } Please pass this message on to the user.
  agent-text-message = { $label } starts at { $time }.
  agent-text-light = Time to wind down: { $label } starts at { $time }.
  agent-text-no-new-work = Time to wrap up: { $label } starts at { $time }.
  agent-text-save-only = Time to stop: { $label } starts at { $time }.
  agent-instruction-light = Until { $until }, finish what is underway and take on only small tasks.
  agent-instruction-no-new-work = Until { $until }, take on nothing new. Help save the work, write down where it stopped and what the next step is, and tell the user to go rest.
  agent-instruction-save-only = Until { $until }, take on nothing new. Help save the work, write down where it stopped and what the next step is, and tell the user to go rest. File edits and shell commands are blocked, except: { $commands }.
  agent-reminder = Reminder from Selflow, the user's own focus tool ({ $label } rule): { $instruction }
  agent-denied = Selflow blocks this until { $until } ({ $label } rule). Only these commands can run: { $commands }. Help the user save the work and stop for the day.
  ```
  `$text` is the step's own message when it has one, else `agent-text-<level>`. A `message` step
  has no instruction: its text goes straight before the closing sentence, with single spaces.

- [ ] **Step 1: Write the failing tests**

`tests/agent.rs`, through `agent_rig` with `no_jitter()` plus a bedtime step and `handle`:
- `a_prompt_gets_the_message_once_then_the_reminder`: light step 60m, tick at 19 22:10; first
  `AgentPrompt` text contains `Time to wind down: bedtime starts at 23:00.` (the fixture has no
  label, so the label is the id) and `finish what is underway`; the second contains
  `Reminder from Selflow` and not `wind down`.
- `a_message_only_step_sends_no_reminder`: level `message`; second prompt `text: None`.
- `a_session_outside_the_folders_gets_nothing`: folders `["~/Workspace/seconv"]`, session in
  `/home/user/notes`: `None`.
- `save_only_denies_edits_and_other_commands`: `Edit` → `Deny` whose reason contains
  `git status, git diff, git add, git commit, git stash push`; `Shell { "git commit -m save" }` →
  `Allow`; `Shell { "rm -rf src" }` → `Deny`; `Other` → `Allow`.
- `the_firmest_level_wins`: light for all, save-only for `~/Workspace/work`: `Edit` denied in
  `/home/user/Workspace/work`, allowed in `/home/user/notes`.
- `the_level_ends_with_the_rule`: tick at 20 07:00, prompt → `None`, `Edit` → `Allow`.
- `a_restart_does_not_resend_a_delivered_message` (Review Focus 5): after the first prompt, build a
  second rig on the same store at the same time (a restarted daemon); its first prompt for the same
  session holds the reminder only.
- `an_unattended_session_gets_nothing_unless_the_step_says_so`.

`tests/engine.rs`:
- `the_engine_publishes_active_agent_steps`: directives empty at 19 21:59, one key at 19 22:00.
- `an_agent_step_records_its_window_as_started`: a rule with only a step: after its time,
  `night_log` holds its window.

`tests/store.rs`: `an_agent_delivery_is_claimed_once`: `true`, then `false`; after
`release_agent_delivery`, `true` again.

Platform: `home_dir_from(Some("/home/u"))` is `Some("/home/u")`; `None` and `Some("")` are `None`.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflowd --test agent --test engine --test store -p selflow-platform --test paths`
Expected: FAIL.

- [ ] **Step 3: Implement** as described. `app.rs` builds the hub from
`enforcer.directives()` and `home_dir()` (log a warning when `None`; `~` folders then cover
nothing). `handle` sends the four agent requests to the hub before taking the store lock.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS, with Task 7's empty-answer test still passing.

- [ ] **Step 5: Commit**

`chore: answer agent hooks from the active steps in the daemon`

---

### Task 9: Agent events in the report

**Files:**
- Modify: `crates/selflowd/src/agent/mod.rs`, `crates/selflowd/src/store/{types.rs,mod.rs}`,
  `crates/selflowd/src/ipc/handlers.rs` (`report_entry`)
- Modify: `locales/{en,pt-BR}/report.ftl`
- Test: `crates/selflowd/tests/agent.rs`, `crates/selflow/tests/render.rs`

**Interfaces:**
- Produces: `EventKind::{AgentMessage, AgentDenied, AgentUnattended}` stored as `agent_message`,
  `agent_denied`, `agent_unattended`. Detail JSON: `label`, `level` (the level's name), `folder`
  (session cwd with the hub's home shown as `~`), plus `how` (`inbox`, `prompt`, `missed`) or `tool`
  (`edit`, `shell`). Keys: messages `{step_key}/{session_id}`, unattended the same, denials
  `{step_key}/{session_id}/{at}`.
- Report ids, each with `label`, `level`, `folder`: `report-agent-message-inbox`,
  `report-agent-message-prompt`, `report-agent-message-missed`, `report-agent-denied-edit`,
  `report-agent-denied-shell`, `report-agent-unattended`. The `.ftl` turns `$level` into the
  level's display name with a select (`Message only`, `Light tasks only`, `No new work`,
  `Save only`).

When each is written: a message event right after its claim (`prompt`; Task 10 adds `inbox`);
`missed` from `ended`, claiming every active covering step the session never got; a denial with
each `Deny`; an unattended event when an unattended session registers in a folder that a step would
cover for an attended one (once per step and session).

- [ ] **Step 1: Write the failing tests**

- `a_message_delivered_with_a_prompt_is_reported`: `events_for(2026-11-19)` holds one
  `AgentMessage` whose report entry is `report-agent-message-prompt` with
  `folder = "~/Workspace/seconv"`.
- `a_denial_is_reported_without_the_command`: entry `report-agent-denied-shell`; no argument holds
  the command text.
- `a_session_that_ends_first_is_reported_as_missed`.
- `an_unattended_run_during_a_level_is_reported_once`: two prompts, one event.
- Render: `every_agent_report_line_renders_in_both_languages`: each id with
  `label = "Bedtime"`, `level = "save-only"`, `folder = "~/w"` contains `Bedtime`, `~/w` and the
  localized level name.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflowd --test agent -p selflow --test render`
Expected: FAIL.

- [ ] **Step 3: Implement.** Event writes use the existing `record_event` and log on failure.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`chore: list agent messages, denials and unattended runs in the report`

---

### Task 10: Inbox delivery (only if Task 1 found a framing)

If "Experiment results" says no framing, skip this task: messages reach sessions on their next
prompt (Task 8), which spec §21 already describes. Record the skip in "Order".

**Files:**
- Create: `crates/selflowd/src/agent/claude_code.rs`, `crates/selflowd/src/agent/courier.rs`
- Modify: `crates/selflowd/src/agent/{mod.rs,constants.rs}`, `crates/selflowd/src/app.rs`
- Test: `crates/selflowd/tests/agent.rs`

**Interfaces:**
- Consumes: Task 8's hub, claims, `Directives`; Task 9's events.
- Produces:
  `pub fn inbox_frame(text: &str) -> Vec<u8>` in `agent::claude_code` (exactly the recorded
  framing with the text substituted, JSON-escaped when the framing is JSON),
  `pub async fn deliver(inbox: &Path, frame: &[u8]) -> io::Result<()>` (through
  `selflow_platform::transport::connect`, write, shutdown, within `INBOX_TIMEOUT = 2 s`), and
  `pub async fn run_courier(hub: Arc<AgentHub>, clock: Arc<dyn Clock>)`, spawned in `app.rs` next to
  `serve`. The courier is the only code that matches on `AgentKind` to pick an adapter.

Each pass (on a directives change or `changed()`): for every registered session with an inbox and
every active step covering it, claim `inbox` first, then deliver; on success record the
`report-agent-message-inbox` event; on failure release the claim (the next prompt delivers it),
and when the socket path no longer exists or refuses connections, remove the session.

- [ ] **Step 1: Write the failing tests**

- `the_frame_matches_the_recorded_sample`: `inbox_frame("hello from selflow")` equals the ledger's
  bytes.
- `an_idle_session_gets_the_message_through_its_inbox`: a `tokio::net::UnixListener` in a temp dir
  stands in for the inbox; register a session with that path, publish an active step: the listener
  reads one frame holding the message text; a later prompt holds only the reminder; the report has
  `report-agent-message-inbox`.
- `a_gone_inbox_leaves_the_message_for_the_next_prompt`: inbox path that doesn't exist: the
  session is dropped from `sessions()`, and its next `AgentPrompt` (which registers it again) holds
  the message.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflowd --test agent`
Expected: FAIL.

- [ ] **Step 3: Implement** as described.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS.

- [ ] **Step 5: Commit**

`chore: deliver agent messages through the Claude Code inbox`

---

### Task 11: `selflow claude hook`, `install` and `uninstall`

**Files:**
- Create: `crates/selflow/src/claude/{mod.rs,types.rs,constants.rs,hook.rs,settings.rs}`; modify
  `src/lib.rs`
- Create: `crates/selflow/src/cli/commands/claude_cmd.rs`; modify `cli/commands/mod.rs`,
  `cli/types.rs`, `cli/mod.rs`
- Modify: `crates/selflow/src/os/{mod.rs,unix.rs}` (settings path, `claude --version`)
- Modify: `crates/selflow-client/src/client/{mod.rs,types.rs,restart.rs}` (`NeverRestart`)
- Modify: `Cargo.toml` (`serde_json` `preserve_order`)
- Modify: `locales/{en,pt-BR}/cli.ftl`
- Test: create `crates/selflow/tests/claude_hook.rs`, `crates/selflow/tests/claude_settings.rs`;
  modify `crates/selflow-client/tests/client.rs`

**Interfaces:**
- Consumes: Task 1's fixtures and results (attended rule, output shapes), Task 7's protocol.
- Produces:
  ```rust
  // selflow_client::client
  pub struct NeverRestart;  // installed_version() -> None, restart() -> false
  // selflow::cli
  Command::Claude(ClaudeArgs)
  pub struct ClaudeArgs { #[command(subcommand)] pub action: ClaudeAction }
  pub enum ClaudeAction { Hook { event: HookEvent }, Install { #[arg(long)] yes: bool }, Uninstall { #[arg(long)] yes: bool } }
  #[derive(Clone, Copy, ValueEnum)] pub enum HookEvent { SessionStart, SessionEnd, Prompt, Tool }
  pub fn claude_cmd(args: ClaudeArgs) -> Result<ExitCode, CliError>;
  // selflow::claude
  pub struct HookEnv { pub inbox: Option<String>, pub attended: Option<String>, pub entrypoint: Option<String> }
  pub fn attended(env: &HookEnv) -> bool;
  pub fn hook_request(event: HookEvent, input: &serde_json::Value, env: &HookEnv) -> Option<Request>;
  pub fn hook_output(event: HookEvent, response: &Response) -> Option<String>;
  pub fn hook_with(event: HookEvent, input: impl Read, env: &HookEnv, socket: Option<&Path>) -> Option<String>;
  pub fn install(settings: &mut serde_json::Value, selflow: &str) -> Vec<HookEdit>;
  pub fn uninstall(settings: &mut serde_json::Value) -> Vec<HookEdit>;
  pub fn delivery_warnings(settings: &serde_json::Value) -> Vec<DeliveryWarning>;
  pub fn parse_claude_version(output: &str) -> Option<(u32, u32, u32)>;
  pub enum HookEdit { Added { event: &'static str, command: String }, Removed { event: String, command: String } }
  pub enum DeliveryWarning { Held { value: String }, Bypassed }
  // selflow::os
  pub fn claude_settings_path_from(config_dir: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf>;
  pub fn claude_settings_path() -> Option<PathBuf>;
  pub fn claude_version_output() -> Option<String>;
  ```
- Hook mapping: `SessionStart` → `AgentSessionStarted`, `SessionEnd` → `AgentSessionEnded`,
  `UserPromptSubmit` → `AgentPrompt`, `PreToolUse` → `AgentTool` with `Edit` for `Edit`, `Write`,
  `MultiEdit`, `NotebookEdit`, `Shell { command: tool_input.command }` for `Bash`, else `Other`.
  The session comes from `session_id` and `cwd` in stdin and `inbox` from
  `CLAUDE_CODE_MESSAGING_SOCKET`; `attended` follows the ledger's rule. Output, from the documented
  shapes the ledger confirms: an `AgentContext` text becomes
  `{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":TEXT}}`, a `Deny`
  becomes
  `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":REASON}}`;
  anything else prints nothing. `hook_with` runs `exchange_with(.., &NeverRestart)` on a
  current-thread runtime inside `tokio::time::timeout(HOOK_DEADLINE, ..)`; the command always exits
  0.
- Settings: Selflow adds one group per event to `hooks.SessionStart`, `hooks.SessionEnd`,
  `hooks.UserPromptSubmit` and `hooks.PreToolUse` (matcher `Edit|Write|MultiEdit|NotebookEdit|Bash`),
  each `{"type": "command", "command": "<selflow> claude hook <event>", "timeout": 5}` where
  `<selflow>` is the running binary's absolute path (`current_exe`), single-quoted when it holds
  whitespace (planner's call). A hook entry is Selflow's when its command's last three words are
  `claude hook <event>` and the program before them is named `selflow`; install replaces Selflow's
  entries whose command differs and adds missing ones; uninstall removes Selflow's entries, then
  groups left empty, then events left empty. `delivery_warnings`: `Held` when
  `crossSessionInbound` (key and location as the ledger records) is `hold` or `refuse`; `Bypassed`
  when `permissions.defaultMode` is `bypassPermissions` and it isn't `accept`.
- `install`/`uninstall` command flow: read the file (missing means `{}`; invalid JSON is an error
  naming the file, nothing written), print each `HookEdit` as `+ <event>: <command>` or
  `- <event>: <command>`, print the version warning (missing `claude`, unparsable, or below
  2.1.224) and delivery warnings, ask `[y/N]` unless `--yes` (through the existing `Prompt` trait),
  write through a temp file in the same folder renamed over the original, keeping the original's
  permissions. No edits means a "nothing to change" line and no write.

- [ ] **Step 1: Write the failing tests**

`tests/claude_hook.rs`, from the fixtures:
- `each_fixture_becomes_its_request`: `session-start.json` with an `inbox` env → 
  `AgentSessionStarted` with the fixture's `session_id`, `cwd` and inbox; `tool-bash.json` →
  `AgentTool` with `Shell { command: "git status" }`; `tool-write.json` → `Edit`;
  `session-end.json` → `AgentSessionEnded`.
- `attended_follows_the_recorded_signal`: the values the ledger recorded for the interactive
  session give `true`, those of `-p` give `false` (or both `true` under the fallback).
- `a_context_answer_becomes_additional_context` and `no_text_prints_nothing`.
- `a_denial_becomes_a_pre_tool_use_deny` and `an_allow_prints_nothing`.
- Review Focus 2: `an_unreachable_daemon_lets_the_call_through` (no socket file: `None`),
  `a_daemon_that_never_answers_lets_the_call_through_in_time` (a listener that accepts and stays
  silent: `None` in under 1.5 s), `a_daemon_on_another_version_lets_the_call_through` (answers
  `SLFW` + `999u32`: `None`), `input_that_is_not_json_lets_the_call_through`.
- Client: `never_restart_never_restarts`.

`tests/claude_settings.rs` (Review Focus 3), on `serde_json::Value` and through the command with a
temp `CLAUDE_CONFIG_DIR`:
- `install_adds_one_group_per_event_and_keeps_the_users_hooks`: start from a copy of the owner's
  shape (an `env` key, then `hooks` with a `UserPromptSubmit` group of three commands and two
  `PreToolUse` groups, then `statusLine`): after install the user's groups are byte-for-byte the
  same and first, Selflow's four groups follow, top-level key order is unchanged.
- `installing_twice_changes_nothing`: the second call returns no edits and the same value.
- `installing_from_another_path_replaces_the_old_entries`: entries for `/old/bin/selflow` become
  the new path, no duplicates; edits list one `Removed` and one `Added` per event.
- `uninstall_removes_only_selflows_entries`: back to the user's original value.
- `a_missing_file_gets_created` and `an_empty_file_counts_as_empty_settings`.
- `invalid_json_is_left_untouched`: the command fails and the file's bytes are unchanged.
- `declining_writes_nothing`: answer `n`, file unchanged; `--yes` writes.
- `delivery_warnings_cover_held_and_bypassed_inbound`: `hold` and `refuse` warn, `accept` doesn't;
  `bypassPermissions` without `accept` warns; `auto` doesn't.
- `the_claude_version_is_read`: `"2.1.332 (Claude Code)"` → `(2, 1, 332)`; `"garbage"` → `None`.
- `the_settings_path_follows_claude_config_dir`: `claude_settings_path_from(Some("/c"), Some("/h"))`
  is `/c/settings.json`; with `None` it is `/h/.claude/settings.json`.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p selflow --test claude_hook --test claude_settings -p selflow-client`
Expected: FAIL.

- [ ] **Step 3: Implement** as specified; CLI strings in `cli.ftl` for the edit heading, the
confirmation, written, nothing to change, invalid file (`path`, `detail`), Claude Code missing,
version too old (`found`, `min`), held inbound (`value`) and bypassed prompts, in both languages.

- [ ] **Step 4: Run**

Run: `make check`
Expected: PASS, and `selflow --help` lists `claude`.

- [ ] **Step 5: Commit**

`feat: connect Claude Code sessions with selflow claude`

---

### Task 12: Isolated scenario, latency, README

The roadmap's "Done when", run by the executor inside the isolated environment with Claude Code
sessions it starts itself.

**Files:**
- Modify: `.claude/skills/testing-selflow-in-isolation/scripts/isolated-env.sh`
- Create: `.claude/skills/testing-selflow-in-isolation/scripts/scenario-claude.sh`
- Modify: `.claude/skills/testing-selflow-in-isolation/SKILL.md`
- Modify: `README.md`, `docs/superpowers/plans/2026-11-18-selflow-phase-1-roadmap.md` (M2b row),
  this plan's Status line

- [ ] **Step 1: Teach the harness Claude Code**

- `up --release`: build with `cargo build --workspace --release` and copy from `target/release`.
- `hook DIR EVENT`: like `run`, but passes `CLAUDE_CODE_MESSAGING_SOCKET`,
  `CLAUDE_CODE_SESSION_ATTENDED` and `CLAUDE_CODE_ENTRYPOINT` through `env -i`, runs
  `selflow claude hook EVENT` in the sandbox, and keeps each call's stdin and stdout as
  `$DIR/claude/calls/<event>-<nanoseconds>.{in,out}.json`.
- `claude-start DIR NAME CWD` / `claude-send DIR NAME TEXT` / `claude-pane DIR NAME` /
  `claude-stop DIR NAME`: Claude Code is not a Selflow binary and needs the owner's login, so it
  runs outside the sandbox, in a tmux server private to the env (`tmux -L sfe-<runtime suffix>`),
  with `--setting-sources project --settings $DIR/claude/settings.json --model haiku`
  (`XDG_RUNTIME_DIR=$RUNTIME` in front when the ledger says the socket follows it). That settings
  file registers `bash <isolated-env.sh> hook <DIR> <event>` for the four events and allows `Write`,
  `Edit`, `Bash(git status:*)` and `Bash(git commit:*)`; `claude-start` gets past first-run dialogs
  with the keys the ledger records.
- `measure-hook DIR N`: inside one sandboxed `bash`, run N `prompt` and N `tool` (Bash
  `git status`) calls with the Task 1 fixtures (cwd rewritten to the covered folder), timing each
  with `$EPOCHREALTIME`, and print p50 and p95 in ms for each.
- `down` also kills the env's tmux server and removes `~/.claude/projects/` folders whose name holds
  the env dir's encoded basename.

- [ ] **Step 2: Write `scenario-claude.sh` (level 1, release build)**

1. `up --level 1 --release`. Folders `$DIR/home/Workspace/seconv` (git repo with one commit) and
   `$DIR/home/Workspace/other`. Config: a lock-free rule `wrap-up`, label `Wrap up seconv`, start
   six minutes ahead, `length = "40m"`, steps `before = "4m"`/`light` and `before = "2m"`/`save-only`,
   both with `folders = ["~/Workspace/seconv"]` (the skill's time rules apply: refuse to start
   between 03:00 and 06:00).
2. Start session A in `seconv` and session B in `other`.
3. At the light step: with inbox delivery, A's pane shows the agent passing on a text with
   `Wrap up seconv` within 120 s; without it, the first prompt to A ("What should I do now?") does.
   The report holds `report-agent-message-inbox` or `-prompt` accordingly.
4. Prompt A again: its latest `prompt` call output holds the reminder.
5. After the save-only step, prompt A: "Create a file named later.txt, then run exactly:
   git commit --allow-empty -m save". A `tool` output denies the write; `git -C seconv log -1
   --format=%s` is `save`; the report holds `report-agent-denied-edit`.
6. Every one of B's call outputs is empty and no report line names `~/Workspace/other`.
7. `measure-hook $DIR 300` while save-only holds: p95 at most 10 ms for both prompt and tool.
8. `down`; `blocked.log` is empty. Print `PASS`.

- [ ] **Step 3: Run the scenarios**

Run: `bash .claude/skills/testing-selflow-in-isolation/scripts/scenario-claude.sh`, then
`scenario-level1.sh` and `scenario-level2.sh` (regressions), then `measure-idle` on a fresh
level-1 release env for 600 s.
Expected: three `PASS` lines; idle CPU close to 0% and RSS in the low tens of MB (spec §15),
compared with the 8.3 MB of the IPC plan. Record the p95 numbers and the idle numbers in the
Status line.

- [ ] **Step 4: Docs**

README: a "Claude Code sessions" section: `selflow claude install` (what it edits, that it asks
first, `uninstall`), an agent step example with the four levels, `[agent] allowed_commands`, that
changes follow the loosening rule, and the inbox behaviour as Task 1 found it. SKILL.md: the new
commands and the Claude Code safety rule (sessions only through `claude-start`). Roadmap: the M2b
row points at this plan. Humanizer pass on the README text. Status line: done, with commits,
measurements and anything left for later.

- [ ] **Step 5: Run and commit**

Run: `make check`
Expected: PASS.

`chore: drive Claude Code sessions in the isolated environment` (harness, scenario, SKILL.md) and
`docs: describe Claude Code sessions` (README, roadmap, this plan), as two commits.

---

## Owner's steps after the run

These touch the real machine and the owner's own Claude Code settings, which agents may not.

1. `make install`, then `systemctl --user restart selflowd.service`, outside lock and shutdown hours.
2. `selflow claude install`, read the listed changes, answer `y`.
3. Add agent steps to the bedtime rule with `selflow config edit` (the default steps only reach new
   configurations), and check the next wind-down in a real session.
