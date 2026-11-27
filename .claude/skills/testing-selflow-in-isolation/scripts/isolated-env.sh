#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH=$(readlink -f "${BASH_SOURCE[0]}")
SCRIPT_DIR=$(dirname "$SCRIPT_PATH")
REPO_ROOT=$(cd "$SCRIPT_DIR/../../../.." && pwd)
STUBBED_COMMANDS=(systemctl loginctl shutdown poweroff reboot halt dbus-update-activation-environment)
HOST_DISPLAY=wayland-host

usage() {
	cat <<'EOF'
Usage: isolated-env.sh <command> [args]

  up [--level 1|2] [--base DIR] [--ttl SECONDS] [--no-build]
      Build, copy the binaries into a fresh dir under the base, start a detached
      supervisor that runs selflowd (and, at level 2, an invisible headless
      mutter with a nested Hyprland inside it), wait until it is ready, and print
      the env dir on stdout. Defaults: level 1, base $SELFLOW_ISO_BASE or $TMPDIR
      or /tmp, ttl 3600. The runtime dir (sockets) is a short /tmp/sfe.XXXXXX.
  down DIR                  Stop everything the supervisor started.
  run DIR CMD [ARGS...]     Run CMD with the isolated environment (env -i + bwrap).
  config-edit DIR FILE      Run `selflow config edit`, saving FILE as the new text.
  restart-daemon DIR        Stop selflowd and start it again on the same store.
  wait-log DIR REGEX [TIMEOUT] [FROM_LINE]
                            Wait until a daemon.log line after FROM_LINE matches.
  log-lines DIR             Print how many lines daemon.log has now.
  lock-pids DIR             List the selflow-ui processes the daemon started.
  screenshot DIR [OUT.png]  Level 2: grim the nested Hyprland.
  hyprctl DIR [ARGS...]     Level 2: hyprctl against the nested Hyprland.
  measure-idle DIR [SECS]   RSS and CPU of selflowd over a window (default 10s).
  env DIR                   Print the isolated variables as export lines.
EOF
}

die() {
	printf 'isolated-env: %s\n' "$*" >&2
	exit 1
}

note() {
	printf '[%(%H:%M:%S)T] %s\n' -1 "$*"
}

state_dir() {
	local dir=${1:-}
	[[ -n $dir ]] || die "missing the env dir"
	[[ -f $dir/level ]] || die "$dir is not an isolated env"
	readlink -f "$dir"
}

sandbox_args() {
	local masked=(
		"/run/user/$(id -u)"
		/run/dbus
		/run/systemd
		/dev/input
		"$HOME/.local/state/selflow"
		"$HOME/.config/selflow"
		"$HOME/.config/hypr"
		"$HOME/.cargo/bin"
	)
	local path
	SANDBOX=(/usr/bin/bwrap --dev-bind / / --die-with-parent)
	for path in "${masked[@]}"; do
		[[ -e $path ]] && SANDBOX+=(--tmpfs "$path")
	done
	return 0
}

load_env() {
	DIR=$(state_dir "${1:-}")
	LEVEL=$(<"$DIR/level")
	RUNTIME=$(<"$DIR/runtime")
	NESTED=""
	[[ -f $DIR/nested ]] && NESTED=$(<"$DIR/nested")
	ISO_PATH="$DIR/bin:$DIR/stubs:/usr/bin"
	ISO_ENV=(
		"HOME=$DIR/home"
		"XDG_RUNTIME_DIR=$RUNTIME"
		"XDG_STATE_HOME=$DIR/state"
		"XDG_CONFIG_HOME=$DIR/config"
		"XDG_CACHE_HOME=$DIR/cache"
		"XDG_DATA_HOME=$DIR/data"
		"SELFLOW_POWER=log"
		"PATH=$ISO_PATH"
		"LANG=C.UTF-8"
		"TERM=dumb"
	)
	[[ -n $NESTED ]] && ISO_ENV+=("WAYLAND_DISPLAY=$NESTED")
	sandbox_args
}

runtime_looks_ours() {
	[[ $1 =~ /sfe\.[A-Za-z0-9]{6}$ && -d $1 && ! -L $1 && -O $1 && $1 != /run/user* ]]
}

assert_isolation() {
	local ui
	runtime_looks_ours "$RUNTIME" || die "XDG_RUNTIME_DIR $RUNTIME is not this env's private sfe.* dir"
	[[ $(stat -c %a "$RUNTIME") == 700 ]] || die "$RUNTIME is not mode 700"
	ui=$(PATH=$ISO_PATH command -v selflow-ui || true)
	if [[ $LEVEL == 1 ]]; then
		[[ -z $ui ]] || die "level 1 must not reach selflow-ui, found $ui"
		[[ -z $NESTED ]] || die "level 1 must not have a WAYLAND_DISPLAY"
		return 0
	fi
	[[ $ui == "$DIR/bin/selflow-ui" ]] || die "selflow-ui resolves to '$ui', not the temp copy"
	[[ -n $NESTED ]] || die "no nested compositor socket recorded"
	[[ $NESTED != */* && $NESTED != *run/user* ]] || die "WAYLAND_DISPLAY '$NESTED' is not a bare socket name"
	[[ $NESTED != "$HOST_DISPLAY" ]] || die "WAYLAND_DISPLAY points at the headless host, not the nested Hyprland"
	[[ -S $RUNTIME/$NESTED ]] || die "nested socket $RUNTIME/$NESTED does not exist"
}

pid_alive() {
	[[ -n ${1:-} ]] && kill -0 "$1" 2>/dev/null
}

wait_gone() {
	local pid=$1 seconds=$2 waited=0
	while pid_alive "$pid" && ((waited < seconds * 10)); do
		sleep 0.1
		waited=$((waited + 1))
	done
	! pid_alive "$pid"
}

read_pid() {
	[[ -f $DIR/$1 ]] && cat "$DIR/$1"
	return 0
}

ours_pids() {
	local entry
	for entry in /proc/[0-9]*; do
		grep -qzxF "XDG_RUNTIME_DIR=$RUNTIME" "$entry/environ" 2>/dev/null && printf '%s\n' "${entry#/proc/}"
	done
	return 0
}

our_pid_named() {
	local pid
	while read -r pid; do
		[[ $(cat "/proc/$pid/comm" 2>/dev/null) == "$1" ]] && printf '%s\n' "$pid" && return 0
	done < <(ours_pids)
	return 1
}

kill_list() {
	local signal=$1 pid
	shift
	for pid in "$@"; do
		[[ -n $pid ]] || continue
		kill "-$signal" "$pid" 2>/dev/null || true
	done
}

log_match() {
	awk -v from="$1" -v re="$2" 'NR > from && $0 ~ re { print; found = 1; exit } END { exit !found }' "$DIR/daemon.log"
}

write_dbus_config() {
	cat >"$DIR/dbus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:dir=$RUNTIME</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
}

write_hypr_config() {
	cat >"$DIR/hypr.lua" <<'EOF'
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
EOF
}

write_stubs() {
	local name
	mkdir -p "$DIR/stubs"
	for name in "${STUBBED_COMMANDS[@]}"; do
		cat >"$DIR/stubs/$name" <<EOF
#!/bin/sh
printf '%s blocked: %s %s\n' "\$(date +%T)" "$name" "\$*" >>"$DIR/blocked.log"
exit 1
EOF
		chmod +x "$DIR/stubs/$name"
	done
}

build_binaries() {
	command -v proto >/dev/null || die "proto is not on PATH; it runs the pinned toolchain"
	(cd "$REPO_ROOT" && proto exec rust -- cargo build --workspace) >&2
}

verify_hypr_config() {
	mkdir -m 700 "$DIR/verify-runtime"
	env -i "HOME=$DIR/home" "XDG_RUNTIME_DIR=$DIR/verify-runtime" "PATH=/usr/bin" \
		"${SANDBOX[@]}" Hyprland --verify-config --config "$DIR/hypr.lua" >"$DIR/hypr-verify.log" 2>&1 ||
		die "the nested Hyprland config does not verify (see $DIR/hypr-verify.log)"
	grep -q 'config ok' "$DIR/hypr-verify.log" || die "the nested Hyprland config does not verify (see $DIR/hypr-verify.log)"
}

cmd_up() {
	local level=1 base=${SELFLOW_ISO_BASE:-${TMPDIR:-/tmp}} ttl=3600 build=1 waited=0 binary
	while (($#)); do
		case $1 in
		--level) level=$2 && shift ;;
		--base) base=$2 && shift ;;
		--ttl) ttl=$2 && shift ;;
		--no-build) build=0 ;;
		*) die "unknown option $1" ;;
		esac
		shift
	done
	[[ $level == 1 || $level == 2 ]] || die "level must be 1 or 2"
	command -v bwrap >/dev/null || die "bubblewrap (bwrap) is required"
	mkdir -p "$base"
	base=$(readlink -f "$base")
	[[ $base != /run/user* ]] || die "the base dir must not live under /run/user"
	((build)) && build_binaries
	DIR=$(mktemp -d "$base/selflow-iso.XXXXXX")
	mkdir -p "$DIR"/{bin,home,state,config,cache,data,screenshots}
	mktemp -d /tmp/sfe.XXXXXX >"$DIR/runtime"
	printf '%s\n' "$level" >"$DIR/level"
	printf '%s\n' "$ttl" >"$DIR/ttl"
	for binary in selflowd selflow; do
		cp "$REPO_ROOT/target/debug/$binary" "$DIR/bin/"
	done
	[[ $level == 2 ]] && cp "$REPO_ROOT/target/debug/selflow-ui" "$DIR/bin/"
	touch "$DIR/daemon.log" "$DIR/notifications.log"
	load_env "$DIR"
	write_stubs
	write_dbus_config
	[[ $level == 2 ]] && write_hypr_config && verify_hypr_config
	setsid -f /usr/bin/bash "$SCRIPT_PATH" _supervise "$DIR" >"$DIR/supervisor.log" 2>&1 </dev/null
	while [[ ! -f $DIR/ready && ! -f $DIR/failed ]] && ((waited < 900)); do
		sleep 0.1
		waited=$((waited + 1))
	done
	[[ -f $DIR/failed ]] && die "start failed: $(<"$DIR/failed") (see $DIR/supervisor.log)"
	[[ -f $DIR/ready ]] || die "start timed out (see $DIR/supervisor.log)"
	printf 'isolated env ready: level %s, XDG_RUNTIME_DIR=%s%s\n' "$level" "$RUNTIME" \
		"$([[ -f $DIR/nested ]] && printf ', WAYLAND_DISPLAY=%s' "$(<"$DIR/nested")")" >&2
	printf '%s\n' "$DIR"
}

supervisor_fail() {
	printf '%s\n' "$*" >"$DIR/failed"
	note "failed: $*"
	exit 1
}

graphics_env() {
	GRAPHICS_ENV=(
		"HOME=$DIR/home"
		"XDG_RUNTIME_DIR=$RUNTIME"
		"XDG_CONFIG_HOME=$DIR/config"
		"XDG_CACHE_HOME=$DIR/cache"
		"XDG_DATA_HOME=$DIR/data"
		"XDG_STATE_HOME=$DIR/state"
		"PATH=$DIR/stubs:/usr/bin"
		"LANG=C.UTF-8"
	)
}

wait_socket() {
	local name=$1 session=$2 what=$3 waited=0
	while [[ ! -S $RUNTIME/$name ]] && ((waited < 300)); do
		pid_alive "$session" || supervisor_fail "$what exited at start"
		sleep 0.1
		waited=$((waited + 1))
	done
	[[ -S $RUNTIME/$name ]] || supervisor_fail "$what made no socket in 30s"
}

start_host() {
	local session
	graphics_env
	note "starting the headless mutter host on $HOST_DISPLAY"
	env -i "${GRAPHICS_ENV[@]}" "${SANDBOX[@]}" \
		dbus-run-session --config-file="$DIR/dbus.conf" -- \
		mutter --headless --wayland --no-x11 --wayland-display="$HOST_DISPLAY" --virtual-monitor 1280x720 \
		>>"$DIR/mutter.log" 2>&1 &
	session=$!
	printf '%s\n' "$session" >"$DIR/host-session.pid"
	wait_socket "$HOST_DISPLAY" "$session" "the headless mutter (see $DIR/mutter.log)"
	our_pid_named mutter >"$DIR/host.pid" || true
	note "headless mutter pid $(read_pid host.pid)"
}

nested_socket() {
	local socket
	for socket in "$RUNTIME"/wayland-*; do
		[[ -S $socket && ${socket##*/} != "$HOST_DISPLAY" ]] && printf '%s\n' "${socket##*/}" && return 0
	done
	return 1
}

start_nested() {
	local session waited=0
	graphics_env
	note "starting the nested Hyprland inside $HOST_DISPLAY"
	env -i "${GRAPHICS_ENV[@]}" \
		"WAYLAND_DISPLAY=$HOST_DISPLAY" \
		"HYPRLAND_NO_SD_VARS=1" \
		"HYPRLAND_NO_SD_NOTIFY=1" \
		"HYPRLAND_NO_CRASHREPORTER=1" \
		"${SANDBOX[@]}" \
		dbus-run-session --config-file="$DIR/dbus.conf" -- Hyprland --config "$DIR/hypr.lua" \
		>>"$DIR/hyprland.log" 2>&1 &
	session=$!
	printf '%s\n' "$session" >"$DIR/hypr-session.pid"
	while ! nested_socket >"$DIR/nested" && ((waited < 300)); do
		pid_alive "$session" || supervisor_fail "the nested Hyprland exited (see $DIR/hyprland.log)"
		sleep 0.1
		waited=$((waited + 1))
	done
	[[ -s $DIR/nested ]] || supervisor_fail "the nested Hyprland made no socket in 30s"
	our_pid_named Hyprland >"$DIR/hypr.pid" || true
	NESTED=$(<"$DIR/nested")
	ISO_ENV+=("WAYLAND_DISPLAY=$NESTED")
	sleep 2
	note "nested Hyprland pid $(read_pid hypr.pid) on $NESTED"
}

start_daemon() {
	local session waited=0 from
	assert_isolation
	from=$(wc -l <"$DIR/daemon.log")
	printf -- '----- isolated-env: daemon start %(%F %T)T -----\n' -1 >>"$DIR/daemon.log"
	rm -f "$DIR/daemon.pid"
	env -i "${ISO_ENV[@]}" "${SANDBOX[@]}" \
		dbus-run-session --config-file="$DIR/dbus.conf" -- /usr/bin/bash "$SCRIPT_PATH" _daemon-inner "$DIR" &
	session=$!
	printf '%s\n' "$session" >"$DIR/daemon-session.pid"
	while [[ ! -S $RUNTIME/selflow/daemon.sock ]] && ((waited < 200)); do
		pid_alive "$session" || supervisor_fail "selflowd exited at start (see $DIR/daemon.log)"
		sleep 0.1
		waited=$((waited + 1))
	done
	[[ -S $RUNTIME/selflow/daemon.sock ]] || supervisor_fail "selflowd made no socket in 20s"
	log_match "$from" 'power mode: log' >/dev/null || {
		stop_daemon
		supervisor_fail "selflowd did not report power mode: log"
	}
	printf '%s\n' "$(($(cat "$DIR/daemon.gen" 2>/dev/null || echo 0) + 1))" >"$DIR/daemon.gen"
	note "selflowd pid $(read_pid daemon.pid) (generation $(<"$DIR/daemon.gen"))"
}

stop_daemon() {
	local pid session monitor children=()
	pid=$(read_pid daemon.pid)
	session=$(read_pid daemon-session.pid)
	monitor=$(read_pid monitor.pid)
	[[ -n $pid || -n $session ]] || return 0
	pid_alive "$pid" && mapfile -t children < <(pgrep -P "$pid")
	note "stopping selflowd ${pid:-?} (children: ${children[*]:-none})"
	kill_list TERM "$pid"
	wait_gone "${pid:-$session}" 10 || kill_list KILL "$pid"
	wait_gone "$session" 5 || kill_list KILL "$session"
	kill_list CONT "${children[@]}" "$monitor"
	kill_list TERM "${children[@]}" "$monitor"
	sleep 0.5
	kill_list KILL "${children[@]}" "$monitor"
	rm -f "$DIR/daemon.pid" "$DIR/daemon-session.pid" "$DIR/monitor.pid"
}

stop_graphics() {
	local name pid session
	for name in hypr host; do
		pid=$(read_pid "$name.pid")
		session=$(read_pid "$name-session.pid")
		[[ -n $pid || -n $session ]] || continue
		note "stopping $name ${pid:-?}"
		kill_list TERM "$pid"
		wait_gone "${pid:-$session}" 10 || kill_list KILL "$pid"
		wait_gone "$session" 5 || kill_list KILL "$session"
		rm -f "$DIR/$name.pid" "$DIR/$name-session.pid"
	done
}

kill_stragglers() {
	local left
	mapfile -t left < <(ours_pids)
	((${#left[@]})) || return 0
	note "killing leftovers that carry this env: ${left[*]}"
	kill_list CONT "${left[@]}"
	kill_list TERM "${left[@]}"
	sleep 1
	mapfile -t left < <(ours_pids)
	kill_list KILL "${left[@]}"
}

keep_hypr_log() {
	local log
	for log in "$RUNTIME"/hypr/*/hyprland.log; do
		[[ -f $log ]] && cp "$log" "$DIR/hyprland-instance.log"
	done
	return 0
}

supervisor_cleanup() {
	trap - EXIT TERM INT HUP USR1
	stop_daemon
	stop_graphics
	kill_stragglers
	keep_hypr_log
	if runtime_looks_ours "$RUNTIME"; then
		rm -rf -- "$RUNTIME" || note "couldn't remove $RUNTIME"
	fi
	touch "$DIR/stopped"
	note "stopped"
}

cmd_supervise() {
	local ttl deadline restart=0 session
	load_env "$1"
	[[ -n ${SELFLOW_ISO_TRACE:-} ]] && set -x
	ttl=$(<"$DIR/ttl")
	printf '%s\n' "$$" >"$DIR/supervisor.pid"
	trap supervisor_cleanup EXIT
	trap 'exit 0' TERM INT HUP
	trap 'restart=1' USR1
	[[ $LEVEL == 2 ]] && start_host && start_nested
	start_daemon
	touch "$DIR/ready"
	deadline=$((SECONDS + ttl))
	while ((SECONDS < deadline)); do
		sleep 1 &
		wait $! || true
		if ((restart)); then
			restart=0
			stop_daemon
			start_daemon
			continue
		fi
		session=$(read_pid daemon-session.pid)
		[[ -n $session ]] && ! pid_alive "$session" && {
			note "selflowd exited on its own (see $DIR/daemon.log)"
			rm -f "$DIR/daemon-session.pid"
		}
	done
	note "time to live of ${ttl}s reached"
}

cmd_daemon_inner() {
	local dir=$1
	printf '%s\n' "$$" >"$dir/daemon.pid"
	dbus-monitor "type='method_call',interface='org.freedesktop.Notifications',member='Notify'" \
		>>"$dir/notifications.log" 2>&1 &
	printf '%s\n' "$!" >"$dir/monitor.pid"
	exec "$dir/bin/selflowd" >>"$dir/daemon.log" 2>&1
}

cmd_down() {
	local supervisor left
	load_env "${1:-}"
	supervisor=$(read_pid supervisor.pid)
	if pid_alive "$supervisor"; then
		kill -TERM "$supervisor"
		wait_gone "$supervisor" 30 || die "the supervisor $supervisor did not stop"
	fi
	mapfile -t left < <(ours_pids)
	((${#left[@]} == 0)) || die "processes still carry this env: ${left[*]}"
	runtime_looks_ours "$RUNTIME" && rm -rf -- "$RUNTIME"
	printf 'stopped; logs kept in %s\n' "$DIR" >&2
}

require_running() {
	[[ -d $RUNTIME && -f $DIR/ready && ! -f $DIR/stopped ]] || die "the env is not running"
}

cmd_run() {
	load_env "${1:-}"
	shift
	(($#)) || die "run needs a command"
	require_running
	assert_isolation
	exec env -i "${ISO_ENV[@]}" "${SANDBOX[@]}" "$@"
}

cmd_config_edit() {
	local file
	load_env "${1:-}"
	file=$(readlink -f "${2:?config-edit needs a TOML file}")
	[[ -f $file ]] || die "no such file $file"
	[[ $file != *[[:space:]]* ]] || die "the TOML path must not contain spaces (VISUAL is word-split)"
	require_running
	assert_isolation
	printf 'n\n' | timeout 60 env -i "${ISO_ENV[@]}" "VISUAL=cp -- $file" "${SANDBOX[@]}" selflow config edit
}

cmd_restart_daemon() {
	local supervisor before waited=0
	load_env "${1:-}"
	supervisor=$(read_pid supervisor.pid)
	pid_alive "$supervisor" || die "the supervisor is not running"
	before=$(<"$DIR/daemon.gen")
	kill -USR1 "$supervisor"
	while [[ $(<"$DIR/daemon.gen") == "$before" ]] && ((waited < 300)); do
		sleep 0.1
		waited=$((waited + 1))
	done
	[[ $(<"$DIR/daemon.gen") != "$before" ]] || die "selflowd did not come back in 30s (see $DIR/supervisor.log)"
	printf 'selflowd restarted, pid %s\n' "$(read_pid daemon.pid)" >&2
}

cmd_wait_log() {
	local regex timeout from deadline
	load_env "${1:-}"
	regex=${2:?wait-log needs a regex}
	timeout=${3:-120}
	from=${4:-0}
	deadline=$((SECONDS + timeout))
	while ((SECONDS < deadline)); do
		log_match "$from" "$regex" && return 0
		sleep 1
	done
	printf 'isolated-env: no daemon.log line matched %s in %ss\n' "$regex" "$timeout" >&2
	return 1
}

cmd_log_lines() {
	load_env "${1:-}"
	wc -l <"$DIR/daemon.log"
}

cmd_lock_pids() {
	local pid clients
	load_env "${1:-}"
	pid=$(read_pid daemon.pid)
	pid_alive "$pid" || die "selflowd is not running"
	clients=$(pgrep -d, -P "$pid" -f "^$DIR/bin/selflow-ui") || return 0
	ps -o pid=,stat=,etimes=,args= -p "$clients"
}

cmd_screenshot() {
	local out
	load_env "${1:-}"
	[[ $LEVEL == 2 ]] || die "screenshots need level 2"
	out=${2:-$DIR/screenshots/shot-$(date +%H%M%S).png}
	require_running
	assert_isolation
	timeout 15 env -i "XDG_RUNTIME_DIR=$RUNTIME" "WAYLAND_DISPLAY=$NESTED" "PATH=/usr/bin" \
		"${SANDBOX[@]}" grim "$out"
	printf '%s\n' "$out"
}

cmd_hyprctl() {
	local signature
	load_env "${1:-}"
	shift
	[[ $LEVEL == 2 ]] || die "hyprctl needs level 2"
	require_running
	assert_isolation
	signature=$(find "$RUNTIME/hypr" -mindepth 2 -maxdepth 2 -name .socket.sock -printf '%h\n')
	signature=${signature##*/}
	[[ -n $signature ]] || die "the nested Hyprland has no IPC socket"
	timeout 15 env -i "XDG_RUNTIME_DIR=$RUNTIME" "HYPRLAND_INSTANCE_SIGNATURE=$signature" "PATH=/usr/bin" \
		"${SANDBOX[@]}" hyprctl "$@"
}

cpu_ticks() {
	local stat rest fields
	stat=$(<"/proc/$1/stat")
	rest=${stat##*) }
	read -ra fields <<<"$rest"
	printf '%s\n' "$((fields[11] + fields[12]))"
}

cmd_measure_idle() {
	local pid seconds hz before after rss
	load_env "${1:-}"
	seconds=${2:-10}
	pid=$(read_pid daemon.pid)
	pid_alive "$pid" || die "selflowd is not running"
	hz=$(getconf CLK_TCK)
	before=$(cpu_ticks "$pid")
	sleep "$seconds"
	after=$(cpu_ticks "$pid")
	rss=$(ps -o rss= -p "$pid" | tr -d ' ')
	printf 'pid %s rss %s KiB cpu %s ticks over %ss (CLK_TCK %s) = %s%%\n' "$pid" "$rss" \
		"$((after - before))" "$seconds" "$hz" \
		"$(awk -v t="$((after - before))" -v h="$hz" -v s="$seconds" 'BEGIN { printf "%.3f", t / h / s * 100 }')"
}

cmd_env() {
	local pair
	load_env "${1:-}"
	assert_isolation
	printf '# without bwrap; prefer: %s run %s <cmd>\n' "$SCRIPT_PATH" "$DIR"
	for pair in "${ISO_ENV[@]}"; do
		printf 'export %s=%q\n' "${pair%%=*}" "${pair#*=}"
	done
}

main() {
	local command=${1:-}
	(($#)) && shift
	declare -A commands=(
		["up"]=cmd_up
		["down"]=cmd_down
		["run"]=cmd_run
		["config-edit"]=cmd_config_edit
		["restart-daemon"]=cmd_restart_daemon
		["wait-log"]=cmd_wait_log
		["log-lines"]=cmd_log_lines
		["lock-pids"]=cmd_lock_pids
		["screenshot"]=cmd_screenshot
		["hyprctl"]=cmd_hyprctl
		["measure-idle"]=cmd_measure_idle
		["env"]=cmd_env
		["_supervise"]=cmd_supervise
		["_daemon-inner"]=cmd_daemon_inner
	)
	[[ -n $command && -n ${commands[$command]:-} ]] || {
		usage >&2
		exit 2
	}
	"${commands[$command]}" "$@"
}

main "$@"
