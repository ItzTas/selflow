# Selflow

Selflow helps people with ADHD, autism or both stop, start and switch tasks at the right moments. It isn't a to-do app. It makes time visible, warns you before each transition and enforces the ones that matter, such as locking the screen at bedtime.

Status: early development.

```sh
cargo build --workspace
```

## Install and use

Install the three binaries (`selflowd`, `selflow` and `selflow-ui`) and the user service:

```sh
make install
```

Then add the two lines from `contrib/hyprland.lua` to your Hyprland configuration. Put the `hl.config` line with your other settings. Put the `exec_cmd` line inside your existing `hyprland.start` handler. It imports the session environment into systemd, then restarts the service. The `&&` makes the restart wait until the import has finished. Without it, the service could start without `WAYLAND_DISPLAY` and wouldn't be able to show the lock screen.

```lua
hl.config({ misc = { allow_session_lock_restore = true } })

hl.on("hyprland.start", function()
	hl.exec_cmd("dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE && systemctl --user restart selflowd.service")
end)
```

Once those lines are in place, start the daemon with `systemctl --user start selflowd.service`, or log out and back in.

To update, run `make install` again and let it finish. Then restart the daemon with `systemctl --user restart selflowd.service`. If `make install` stopped or failed, run it again before you restart the service, log out or run `selflow`. When an update changes how the programs talk to each other, the next `selflow` command restarts an outdated daemon by itself, but treat that as a safety net and restart the service anyway. If the installed programs don't match each other, the command says so and exits with code 2. Running `make install` again and restarting the service fixes it. Do updates outside your lock and shutdown hours. The first update from a version without the automatic restart forgets, that one time, which notices the daemon already sent and which shutdowns it already announced.

The first time, run `selflow config edit` and save. No rule runs until that first save. While nothing is saved yet, closing the editor saves the file even if you left it as it was. Updating from the first milestone starts the configuration over (your reports and extensions stay), so run it then too. It opens the new default file with your old settings as comments at the end, if you have any. Set them again there, outside your lock hours. Everything in the first save applies at once, and so does a new lock or shutdown later. Saving a lock during its hours locks the screen right away, and saving a shutdown rule during its hours turns the computer off a minute later.

### The configuration file

The default file explains each setting in comments. In short:

- A rule has an `id`, a `start` and an `end` (an anchor such as `bedtime`, or a time like `"23:00"`). A rule with only a `start` and a `message` sends one notice when it starts.
- `notices` lists how long before the start each notice comes (`["15m", "5m", "1m"]` by default). `message` replaces the text of the notices, and a list of messages picks one each day.
- `lock = true` locks the screen from the start to the end. `shutdown_after = "30m"` turns the computer off if the screen stays locked that long.
- `shutdown = "once"` turns the computer off at the start of the rule instead. `"repeat"` does it again on every boot until the rule ends. A shutdown always comes at least a minute after a notice or the lock screen announced it, and a machine that resumes past a due shutdown gets a fresh minute.
- Unlocking early is a set of challenges: a wait, a reason, a phrase to type and a number of extensions. The rule set has defaults, and a `[rule_set.rule.challenges]` table changes them for one rule.
- An `[[exception]]` skips a date, for the listed rules or for all of them.

Each change in a save keeps its own clock. Stricter and neutral changes apply at once. A change that loosens the rules waits the loosening delay (24 hours by default), counted from the save that asked for it. Putting the old value back cancels the wait. An `[[exception]]` in the file waits like any other loosening, so add it at least a day ahead.

### Commands

- `selflow config status` shows when your settings were last saved and the changes still waiting, each with its date.
- `selflow report` shows what happened on past nights.
- `selflow unlock` answers the challenges of the lock on screen (the wait, the reason, the phrase, then one of the rule's extensions) and frees the screen. Run it from a terminal, for example a TTY while the lock covers your session.
- `selflow exception --rule <id>` skips that rule for today. It goes through the rule's challenges and needs an extension left for it, and it is refused while that rule's lock is on screen or one of its extensions is running. If its wait ends and you don't confirm within 10 minutes, it starts over. A rule that only sends notices is skipped at once.
- `selflow test-lock` starts a short lock to try the screen. `--for 10m` sets the length (5 minutes by default). `--rule` picks the lock rule to borrow, written as `<set>/<id>` (for example `--rule every-night/bedtime`), while `selflow exception --rule` takes the id alone. Without `--rule`, Selflow uses the first lock rule in today's rule set. It only works while the daemon runs in test mode (`SELFLOW_POWER=log`), where a shutdown is logged and never carried out.
- `selflow-ui --preview` shows the lock screen in a normal window, without locking. Escape closes it.

### If another screen locker runs when you're idle

Guard it so it doesn't take over during a Selflow lock. For example, start your locker like this in your idle settings:

```sh
pgrep -f "selflow-ui [-]-lock" >/dev/null || <your locker command>
```

The brackets keep the pattern from matching the shell that runs this line. Without them, `pgrep` finds that shell every time and your locker never starts.

### If a lock screen stays up after the lock ended

Switch to a TTY, log in and run `selflow unlock`. With nothing locked, it releases the stuck screen at once. If the lock client has died, releasing the screen relies on `misc:allow_session_lock_restore = true`, which is why the Hyprland line above is there.

## Claude Code sessions

A rule can also talk to the Claude Code sessions you have open. Before the rule starts, an agent step sends Claude a message, and from then until the rule ends it holds a level that limits what Claude takes on.

### Connecting Claude Code

Run `selflow claude install`. It adds hooks for four Claude Code events (`SessionStart`, `SessionEnd`, `UserPromptSubmit` and `PreToolUse`) to `~/.claude/settings.json`, or to `$CLAUDE_CONFIG_DIR/settings.json` when that variable is set. Each hook runs `selflow claude hook <event>`. The command lists the lines it would add and asks before it writes anything (`--yes` skips the question). Your own hooks stay where they are. If the file isn't valid JSON, or something changed it while the command waited for your answer, nothing is written. Claude Code sessions you start after that pick up the hooks.

It also warns you in three cases. If Claude Code is missing, or older than 2.1.224 and so without an inbox, messages wait for your next prompt. If `crossSessionInbound` is set to anything but `"accept"`, messages wait for your approval or never arrive. If your default permission mode is `bypassPermissions` and `crossSessionInbound` isn't `"accept"`, Claude Code holds messages until you approve them. `selflow claude uninstall` removes Selflow's hooks and leaves the rest of the file alone.

The hooks never hold Claude Code up. If the daemon isn't running, takes more than a second to answer or runs a different version, the hook exits quietly and the session carries on as if Selflow weren't there.

### Agent steps

Steps go in `[[rule_set.rule.agent]]` tables under their rule:

```toml
[[rule_set.rule]]
id = "bedtime"
label = "Bedtime"
start = "bedtime"
end = "wake"
lock = true

[[rule_set.rule.agent]]
before = "60m"
level = "light"
folders = ["~/Workspace/work"]

[[rule_set.rule.agent]]
before = "15m"
level = "save-only"
message = "Bedtime soon. Save the work and stop here."
```

- `before` says how long before the rule's start the step happens. It has to be whole minutes and under 24 hours.
- `level` is one of four, from lightest to firmest. With `message`, Claude gets the message and passes it on to you, and nothing else changes. With `light`, every prompt you send also carries a reminder to finish what is underway and take on only small tasks. With `no-new-work`, the reminder asks Claude to start nothing new, help save the work, write down where it stopped and what comes next, and tell you to go rest. `save-only` sends the same reminder and also blocks file edits and shell commands, except the allowed commands below.
- `message` replaces Selflow's own text for that step.
- `folders` limits the step to sessions working inside those folders. Write `~`, a path starting with `~/`, or an absolute path. A folder covers everything inside it, but `~/Workspace/app` doesn't cover `~/Workspace/app-old`. Claude Code reports the working directory with symlinks resolved, so write a folder behind a symlink as its real path. Without `folders`, the step covers every session.
- `unattended = true` also covers runs nobody is watching, such as `claude -p`. Selflow leaves those alone by default, and the report lists the ones that start while a level holds.

A level holds from its step until the rule ends. When several steps cover a session, the firmest one governs it. Each session gets each message once, including a session you open after the step and one the daemon only learns about after a restart.

At `save-only`, Claude can still run the commands in the `[agent]` table. This is the default list:

```toml
[agent]
allowed_commands = ["git status", "git diff", "git add", "git commit", "git stash push"]
```

A shell command passes when its first words match an entry, it has no shell operator (`;`, `&`, `|`, `>`, `<`, `$`, backticks, parentheses, braces or a line break), and none of its arguments starts with `--output`. So `git commit -m "wip"` runs, and `git commit -m wip && rm -rf src` is blocked.

Agent steps follow the same clocks as the rest of the file. Adding a step, covering more folders, turning `unattended` on or allowing fewer commands applies at once. Removing a step, covering fewer folders, turning `unattended` off or allowing more commands waits the loosening delay. Selflow tells steps apart by their `before` and `level`, so changing either one adds a new step right away and removes the old one after the delay. A new `message` applies at once.

### How messages arrive

Selflow sends each message to the session's inbox, the local socket Claude Code opens for messages from other sessions. This needs Claude Code 2.1.224 or later. A session sitting idle gets it too: Claude wakes up and answers. Claude Code shows the message as coming from another Claude session, not as something you typed. Selflow's text says it comes from your focus tool and asks Claude to pass it on to you. When the inbox can't be reached, the message comes with your next prompt instead. If `crossSessionInbound` holds or refuses messages, the text of a `message` step never arrives. Steps at `light` and firmer still remind Claude with each prompt.

`selflow report` lists which way each message went and the edits and commands Selflow blocked, with the rule, the level and the session's folder. Selflow doesn't read transcripts or prompts, and it never stores the commands Claude runs.
