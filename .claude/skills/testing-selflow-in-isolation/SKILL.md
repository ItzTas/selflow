---
name: testing-selflow-in-isolation
description: Use when testing Selflow end to end on the owner's machine (running selflow, selflowd or selflow-ui, checking the lock screen, selflow unlock, a frozen lock client, exceptions, shutdown rules, notices or the idle daemon's RSS/CPU) while the owner's real daemon and Hyprland session are live
---

# Testing Selflow in isolation

## Overview

The owner's real `selflowd` runs as a user service and their Hyprland session is live. A bare `selflow ...` talks to that daemon, a bare `selflow-ui --lock` locks their screen, and a nested Hyprland without `--config` runs their `hyprland.start` hook, which restarts the real daemon. **Every Selflow process in a test goes through `scripts/isolated-env.sh`.** It builds copies of the binaries and runs them under `env -i` plus bubblewrap, with the owner's sockets, store, `~/.cargo/bin`, `~/.config/hypr` and `/dev/input` masked, `SELFLOW_POWER=log`, a private D-Bus, and stubs for `systemctl`/`poweroff`. The owner authorized this harness on 2026-11-26; outside it, the plan's agent-safety rules still apply in full.

## The two levels

| Level | What runs | What it can test |
|---|---|---|
| 1 | isolated `selflowd`, no `selflow-ui` | config saves and clocks, `config status`, notices (recorded in `notifications.log`), lockless shutdowns (`shutdown requested` in `daemon.log`), restarts, exceptions, `report`, idle RSS/CPU |
| 2 | level 1 + `selflow-ui` inside a compositor backend (default `hyprland`: a nested Hyprland inside a headless mutter, no window anywhere) | the lock screen (screenshots), `selflow unlock`, a frozen or killed lock client, release clients, test locks |

Neither level covers the owner's monitors, their Hyprland config, a TTY switch, a real power-off or their real notification daemon: those stay on the owner's checklist.

## Quick reference

```sh
export SELFLOW_ISO_BASE=<your scratchpad dir>
ISO=.claude/skills/testing-selflow-in-isolation/scripts/isolated-env.sh
D=$(bash $ISO up --level 2)                       # builds; prints the env dir; --compositor NAME (default hyprland)
bash $ISO config-edit "$D" /abs/path/rules.toml   # VISUAL="cp -- file"
bash $ISO run "$D" selflow test-lock --for 3m
bash $ISO screenshot "$D" "$D/screenshots/lock.png"   # then Read the PNG
bash $ISO ctl "$D" monitors                       # the backend's control command (hyprctl); `hyprctl` is an alias
printf '%s\n' "a reason of ten chars or more" | timeout 150 bash $ISO run "$D" selflow unlock
bash $ISO lock-pids "$D"                          # selflow-ui children: kill -STOP <pid>
bash $ISO wait-log "$D" 'starting a client to release the screen' 30
bash $ISO restart-daemon "$D"                     # same store, new daemon
bash $ISO measure-idle "$D" 600
bash $ISO down "$D"
```

`bash $ISO` with no arguments prints every command. `scripts/scenario-level1.sh` and `scripts/scenario-level2.sh` are full end-to-end proofs (about 5 minutes each, ending in `PASS`); read them for working examples of every step.

## Driving the CLI

- Prompts read stdin line by line after the CLI's own wait, so pipe all answers up front (reason, then phrase if the rule has one) and wrap in `timeout` (wait + about 10 s). A refused answer is asked again; at EOF the CLI exits with the refusal.
- Rule times: write clock times a few minutes ahead (`date -d '+3 min' +%H:%M`), start before :45 s of the minute, and stay out of 03:00–06:00 (the 05:00 day boundary). `report --date` takes the logical date, `YYYY-MM-DD`.
- Wait for the lock before `selflow unlock`: until `bash $ISO lock-pids "$D"` lists a client (about 20 s after the start minute), `unlock` only answers "Nothing is locked right now" and releases the screen. The extension form opens `wait` after the lock starts.
- Lock rules reach a screen only at level 2. At level 1 the daemon still enforces the lock (unlock, extensions and the report work) but logs `couldn't start the lock client`.
- `daemon.log` timestamps are UTC; the CLI and notices use local time.

## Adding a compositor

`up --level 2 --compositor NAME` sources `scripts/backends/NAME.sh` (NAME matches `^[a-z0-9-]+$`) and records NAME in the env dir, so `screenshot`, `ctl` and the supervisor's cleanup use the same backend. `up` refuses a backend that doesn't define all of these:

- `backend_write_config`: write the compositor's own config into `$DIR`; never read the owner's.
- `backend_verify`: check that config before anything starts, and `die` if it is wrong.
- `backend_start`: start every process with `spawn_compositor NAME LOG [VAR=VALUE...] -- CMD` (env -i, bwrap, private D-Bus), wait until it is ready, then `track_pid NAME`; call `supervisor_fail` on errors. It must open no window in the owner's session and never connect to the owner's compositor.
- `backend_socket`: print the bare name of the Wayland socket the lock client uses, a socket inside `$RUNTIME`.
- `backend_parent_sockets`: print any other socket names it made in `$RUNTIME` (a headless parent, say); the assertions refuse them as `WAYLAND_DISPLAY`.
- `backend_processes`: print the process names (`comm`) to stop, in stop order. The runtime-dir sweep runs after them.
- `backend_screenshot OUT`: write a PNG of the nested screen, under `env -i` and `"${SANDBOX[@]}"`.
- `backend_ctl ARGS...`: run the compositor's control command against the nested instance only.
- `backend_keep_logs`: copy logs out of `$RUNTIME` into `$DIR` before it is removed.

`scripts/backends/hyprland.sh` is the example. KWin, for one, has a virtual backend (`kwin_wayland --virtual`), but `grim` needs wlr-screencopy, which KWin lacks, so its `backend_screenshot` needs another tool.

## Red flags: stop

- About to run `selflow`, `selflowd`, `selflow-ui`, `grim` or `hyprctl` without `bash $ISO run`/`screenshot`/`ctl` in front.
- About to start a Hyprland or mutter yourself, or to use `hyprctl output create` on the owner's session.
- A runtime dir under the scratchpad (sockets are capped at 108 bytes; the harness uses `/tmp/sfe.XXXXXX`).
- `pkill`/`killall` by a bare name, or killing a `bwrap` PID (it doesn't forward signals). Use `down`; it kills only processes carrying this env's runtime dir.
- `blocked.log` in the env dir is not empty: something tried to reach systemd or power off. Stop and tell the owner.
