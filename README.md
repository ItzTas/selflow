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

To update, run `make install` again. The next `selflow` command checks the running daemon against the installed one and restarts the daemon if they differ, so you don't have to restart the service yourself. If the installed programs don't match each other, for example after an install that stopped halfway, nothing is restarted. The command says so and exits with code 2, and running `make install` again fixes it.

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
