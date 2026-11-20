# Selflow

Selflow helps people with ADHD, autism or both stop, start and switch tasks at the right moments. It isn't a to-do app. It makes time visible, warns you before each transition and enforces the ones that matter, such as locking the screen at bedtime.

Status: Milestone 1 in progress.

```sh
cargo build --workspace
```

## Install and use

Install the binaries and the user service:

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

The first time, run `selflow config edit`. It opens the default file in your language. Set your bedtime and wake time there and save. Do this outside your lock hours. A new lock applies as soon as you save it, so saving during its hours locks the screen right away.

Run `selflow config status` to see the version in force and any changes waiting to apply. Run `selflow report` to see what happened on past nights.

`selflow-ui --preview` shows the lock screen in a normal window, without locking.

### If another screen locker runs when you're idle

Guard it so it doesn't take over during a Selflow lock. For example, start your locker like this in your idle settings:

```sh
pgrep -f "selflow-ui --lock" >/dev/null || <your locker command>
```

### If a lock screen stays up after the lock ended

This can happen when the daemon restarts at the exact moment a lock ends. Switch to another TTY, log in and, while `selflowd` is running, start a new lock client:

```sh
hyprctl --instance 0 dispatch exec "selflow-ui --lock"
```

The new client sees that no lock is active and releases the session.
