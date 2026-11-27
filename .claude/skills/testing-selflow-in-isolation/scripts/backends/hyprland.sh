#!/usr/bin/env bash

HYPRLAND_HOST_DISPLAY=wayland-host

backend_write_config() {
	cat >"$DIR/hypr.lua" <<'LUA'
hl.monitor({ output = "", mode = "1280x720", position = "auto", scale = "1" })
hl.config({
    misc = {
        allow_session_lock_restore = true,
        disable_hyprland_logo = true,
        disable_splash_rendering = true,
    },
    ecosystem = {
        no_update_news = true,
        no_donation_nag = true,
    },
})
LUA
}

backend_verify() {
	mkdir -m 700 "$DIR/verify-runtime"
	env -i "HOME=$DIR/home" "XDG_RUNTIME_DIR=$DIR/verify-runtime" "PATH=/usr/bin" \
		"${SANDBOX[@]}" Hyprland --verify-config --config "$DIR/hypr.lua" >"$DIR/hypr-verify.log" 2>&1 ||
		die "the nested Hyprland config does not verify (see $DIR/hypr-verify.log)"
	grep -q 'config ok' "$DIR/hypr-verify.log" || die "the nested Hyprland config does not verify (see $DIR/hypr-verify.log)"
}

hyprland_start_host() {
	local session
	note "starting the headless mutter host on $HYPRLAND_HOST_DISPLAY"
	spawn_compositor mutter "$DIR/mutter.log" -- \
		mutter --headless --wayland --no-x11 --wayland-display="$HYPRLAND_HOST_DISPLAY" --virtual-monitor 1280x720
	session=$(read_pid mutter-session.pid)
	wait_socket "$HYPRLAND_HOST_DISPLAY" "$session" "the headless mutter (see $DIR/mutter.log)"
	track_pid mutter
	note "headless mutter pid $(read_pid mutter.pid)"
}

hyprland_start_nested() {
	local session waited=0
	note "starting the nested Hyprland inside $HYPRLAND_HOST_DISPLAY"
	spawn_compositor Hyprland "$DIR/hyprland.log" \
		"WAYLAND_DISPLAY=$HYPRLAND_HOST_DISPLAY" \
		"HYPRLAND_NO_SD_VARS=1" \
		"HYPRLAND_NO_SD_NOTIFY=1" \
		"HYPRLAND_NO_CRASHREPORTER=1" \
		-- Hyprland --config "$DIR/hypr.lua"
	session=$(read_pid Hyprland-session.pid)
	while ! backend_socket >/dev/null && ((waited < 300)); do
		pid_alive "$session" || supervisor_fail "the nested Hyprland exited (see $DIR/hyprland.log)"
		sleep 0.1
		waited=$((waited + 1))
	done
	backend_socket >/dev/null || supervisor_fail "the nested Hyprland made no socket in 30s"
	track_pid Hyprland
	sleep 2
	note "nested Hyprland pid $(read_pid Hyprland.pid) on $(backend_socket)"
}

backend_start() {
	hyprland_start_host
	hyprland_start_nested
}

backend_socket() {
	local socket
	for socket in "$RUNTIME"/wayland-*; do
		[[ -S $socket && ${socket##*/} != "$HYPRLAND_HOST_DISPLAY" ]] && printf '%s\n' "${socket##*/}" && return 0
	done
	return 1
}

backend_parent_sockets() {
	printf '%s\n' "$HYPRLAND_HOST_DISPLAY"
}

backend_processes() {
	printf '%s\n' Hyprland mutter
}

backend_screenshot() {
	timeout 15 env -i "XDG_RUNTIME_DIR=$RUNTIME" "WAYLAND_DISPLAY=$NESTED" "PATH=/usr/bin" \
		"${SANDBOX[@]}" grim "$1"
}

backend_ctl() {
	local signature
	signature=$(find "$RUNTIME/hypr" -mindepth 2 -maxdepth 2 -name .socket.sock -printf '%h\n')
	signature=${signature##*/}
	[[ -n $signature ]] || die "the nested Hyprland has no IPC socket"
	timeout 15 env -i "XDG_RUNTIME_DIR=$RUNTIME" "HYPRLAND_INSTANCE_SIGNATURE=$signature" "PATH=/usr/bin" \
		"${SANDBOX[@]}" hyprctl "$@"
}

backend_keep_logs() {
	local log
	for log in "$RUNTIME"/hypr/*/hyprland.log; do
		[[ -f $log ]] && cp "$log" "$DIR/hyprland-instance.log"
	done
	return 0
}
