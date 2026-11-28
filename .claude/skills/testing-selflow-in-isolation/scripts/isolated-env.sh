#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH=$(readlink -f "${BASH_SOURCE[0]}")
SCRIPT_DIR=$(dirname "$SCRIPT_PATH")
REPO_ROOT=$(cd "$SCRIPT_DIR/../../../.." && pwd)
STUBBED_COMMANDS=(systemctl loginctl shutdown poweroff reboot halt dbus-update-activation-environment)
DEFAULT_COMPOSITOR=hyprland
BACKEND_FUNCTIONS=(
	backend_write_config
	backend_verify
	backend_start
	backend_socket
	backend_parent_sockets
	backend_processes
	backend_screenshot
	backend_ctl
	backend_keep_logs
)
CLAUDE_HOOKS=(
	"SessionStart session-start"
	"SessionEnd session-end"
	"UserPromptSubmit prompt"
	"PreToolUse tool"
)
CLAUDE_TOOL_MATCHER="Edit|Write|MultiEdit|NotebookEdit|Bash"
CLAUDE_ALLOWED_COMMANDS=("Bash(git status:*)" "Bash(git commit:*)")
CLAUDE_FILE_TOOLS=(Read Edit Write)
CLAUDE_PERMISSION_PROMPT='Esc to cancel|Do you want to |❯ [0-9]+\. Yes'
CLAUDE_START_TIMEOUT=90
CLAUDE_STOP_TIMEOUT=20

usage() {
	cat <<'EOF'
Usage: isolated-env.sh <command> [args]

  up [--level 1|2] [--compositor NAME] [--base DIR] [--ttl SECONDS] [--no-build] [--release]
      Build, copy the binaries into a fresh dir under the base, start a detached
      supervisor that runs selflowd (and, at level 2, the invisible compositor
      from backends/NAME.sh; hyprland runs a nested Hyprland inside a headless
      mutter), wait until it is ready, and print the env dir on stdout. Defaults:
      level 1, compositor hyprland, base $SELFLOW_ISO_BASE or $TMPDIR or /tmp,
      ttl 3600, debug build (--release builds and copies target/release). The
      runtime dir (sockets) is a short /tmp/sfe.XXXXXX.
  down DIR                  Stop the env's Claude Code sessions and everything the
                            supervisor started, and remove the sessions'
                            ~/.claude/projects/ folders.
  run DIR CMD [ARGS...]     Run CMD with the isolated environment (env -i + bwrap).
  config-edit DIR FILE      Run `selflow config edit`, saving FILE as the new text.
  restart-daemon DIR        Stop selflowd and start it again on the same store.
  wait-log DIR REGEX [TIMEOUT] [FROM_LINE]
                            Wait until a daemon.log line after FROM_LINE matches.
  log-lines DIR             Print how many lines daemon.log has now.
  lock-pids DIR             List the selflow-ui processes the daemon started.
  screenshot DIR [OUT.png]  Level 2: screenshot the nested compositor.
  ctl DIR [ARGS...]         Level 2: the compositor's control command (hyprctl
                            for hyprland). `hyprctl` is an alias for now.
  measure-idle DIR [SECS]   RSS and CPU of selflowd over a window (default 10s).
  env DIR                   Print the isolated variables as export lines.

Claude Code (runs outside the sandbox with the owner's login, in a tmux server
private to the env, with only $DIR/claude/settings.json and --model haiku):
  claude-start DIR NAME CWD Start session NAME in CWD (inside DIR), get past the
                            first-run dialogs, wait for its SessionStart hook.
  claude-send DIR NAME TEXT Type TEXT into session NAME and press Enter.
  claude-pane DIR NAME      Print session NAME's screen and scrollback.
  claude-deny DIR NAME      If session NAME shows a permission prompt, refuse it
                            (Escape, which also ends the turn); never approves.
  claude-stop DIR NAME      Send /exit to session NAME and wait for it to close.
  claude-id DIR NAME        Print session NAME's session id.
  claude-calls DIR NAME EVENT
                            List session NAME's kept hook inputs for EVENT
                            (session-start, session-end, prompt, tool), oldest
                            first; the output sits next to each as .out.json.
  hook DIR EVENT            The hook command the sessions run: `selflow claude
                            hook EVENT` in the sandbox, stdin and stdout kept in
                            $DIR/claude/calls/<event>-<ns>.{in,out}.json.
  measure-hook DIR N [CWD]  Time N prompt and N tool (Bash git status) hook calls
                            from the Claude fixtures with cwd set to CWD
                            (default $DIR/home); prints the outputs of an untimed
                            call before and after, then p50 and p95 in ms.
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

backend_path() {
	[[ $1 =~ ^[a-z0-9-]+$ ]] || die "the compositor name '$1' must match ^[a-z0-9-]+\$"
	[[ -f $SCRIPT_DIR/backends/$1.sh ]] || die "no compositor backend $SCRIPT_DIR/backends/$1.sh"
	printf '%s\n' "$SCRIPT_DIR/backends/$1.sh"
}

load_backend() {
	local file function
	COMPOSITOR=$1
	file=$(backend_path "$COMPOSITOR")
	source "$file"
	for function in "${BACKEND_FUNCTIONS[@]}"; do
		declare -F "$function" >/dev/null || die "$file does not define $function"
	done
}

load_paths() {
	DIR=$(state_dir "${1:-}")
	LEVEL=$(<"$DIR/level")
	RUNTIME=$(<"$DIR/runtime")
}

load_env() {
	load_paths "${1:-}"
	COMPOSITOR=""
	[[ $LEVEL != 2 || -f $DIR/compositor ]] || die "$DIR is a level 2 env with no recorded compositor"
	[[ $LEVEL == 2 ]] && load_backend "$(<"$DIR/compositor")"
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
	local ui parent
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
	while read -r parent; do
		[[ $NESTED != "$parent" ]] || die "WAYLAND_DISPLAY points at the $COMPOSITOR parent socket $parent, not the nested compositor"
	done < <(backend_parent_sockets)
	[[ -S $RUNTIME/$NESTED && ! -L $RUNTIME/$NESTED ]] || die "nested socket $RUNTIME/$NESTED does not exist"
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
	write_systemctl_stub
}

write_systemctl_stub() {
	cat >"$DIR/stubs/systemctl" <<EOF
#!/bin/sh
case "\$*" in
"--user show selflowd.service -p ExecStart --value")
	printf '{ path=%s ; argv[]=%s ; }\n' "$DIR/bin/selflowd" "$DIR/bin/selflowd"
	exit 0
	;;
"--user --no-block restart selflowd.service")
	setsid -f /usr/bin/bash "$SCRIPT_PATH" restart-daemon "$DIR" >>"$DIR/restart.log" 2>&1 </dev/null
	exit 0
	;;
esac
printf '%s blocked: systemctl %s\n' "\$(date +%T)" "\$*" >>"$DIR/blocked.log"
exit 1
EOF
	chmod +x "$DIR/stubs/systemctl"
}

build_binaries() {
	local flags=()
	[[ $1 == release ]] && flags=(--release)
	command -v proto >/dev/null || die "proto is not on PATH; it runs the pinned toolchain"
	(cd "$REPO_ROOT" && proto exec rust -- cargo build --workspace "${flags[@]}") >&2
}

cmd_up() {
	local level=1 compositor="" base=${SELFLOW_ISO_BASE:-${TMPDIR:-/tmp}} ttl=3600 build=1 waited=0 binary
	local profile=debug
	while (($#)); do
		case $1 in
		--level) level=$2 && shift ;;
		--compositor) compositor=$2 && shift ;;
		--base) base=$2 && shift ;;
		--ttl) ttl=$2 && shift ;;
		--no-build) build=0 ;;
		--release) profile=release ;;
		*) die "unknown option $1" ;;
		esac
		shift
	done
	[[ $level == 1 || $level == 2 ]] || die "level must be 1 or 2"
	[[ $level == 2 || -z $compositor ]] || die "--compositor needs --level 2"
	[[ $level == 2 ]] && compositor=${compositor:-$DEFAULT_COMPOSITOR} && load_backend "$compositor"
	command -v bwrap >/dev/null || die "bubblewrap (bwrap) is required"
	mkdir -p "$base"
	base=$(readlink -f "$base")
	[[ $base != /run/user* ]] || die "the base dir must not live under /run/user"
	((build)) && build_binaries "$profile"
	DIR=$(mktemp -d "$base/selflow-iso.XXXXXX")
	mkdir -p "$DIR"/{bin,home,state,config,cache,data,screenshots}
	mktemp -d /tmp/sfe.XXXXXX >"$DIR/runtime"
	printf '%s\n' "$level" >"$DIR/level"
	printf '%s\n' "$ttl" >"$DIR/ttl"
	printf '%s\n' "$profile" >"$DIR/profile"
	[[ $level == 2 ]] && printf '%s\n' "$compositor" >"$DIR/compositor"
	for binary in selflowd selflow; do
		cp "$REPO_ROOT/target/$profile/$binary" "$DIR/bin/"
	done
	[[ $level == 2 ]] && cp "$REPO_ROOT/target/$profile/selflow-ui" "$DIR/bin/"
	touch "$DIR/daemon.log" "$DIR/notifications.log"
	load_env "$DIR"
	write_stubs
	write_dbus_config
	[[ $level == 2 ]] && backend_write_config && backend_verify
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

spawn_compositor() {
	local name=$1 log=$2 vars=() graphics_env=(
		"HOME=$DIR/home"
		"XDG_RUNTIME_DIR=$RUNTIME"
		"XDG_CONFIG_HOME=$DIR/config"
		"XDG_CACHE_HOME=$DIR/cache"
		"XDG_DATA_HOME=$DIR/data"
		"XDG_STATE_HOME=$DIR/state"
		"PATH=$DIR/stubs:/usr/bin"
		"LANG=C.UTF-8"
	)
	shift 2
	while (($#)) && [[ $1 != -- ]]; do
		[[ $1 =~ ^[A-Z_][A-Z0-9_]*= && ! $1 =~ ^(HOME|PATH|XDG_[A-Z_]*)= ]] ||
			supervisor_fail "spawn_compositor $name may not set '${1%%=*}'"
		vars+=("$1")
		shift
	done
	(($#)) && shift
	(($#)) || supervisor_fail "spawn_compositor $name needs a command after --"
	env -i "${graphics_env[@]}" "${vars[@]}" "${SANDBOX[@]}" \
		dbus-run-session --config-file="$DIR/dbus.conf" -- "$@" >>"$log" 2>&1 &
	printf '%s\n' "$!" >"$DIR/$name-session.pid"
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

track_pid() {
	our_pid_named "$1" >"$DIR/$1.pid" || true
}

start_compositor() {
	backend_start
	backend_socket >"$DIR/nested" || supervisor_fail "the $COMPOSITOR backend reported no socket"
	NESTED=$(<"$DIR/nested")
	ISO_ENV+=("WAYLAND_DISPLAY=$NESTED")
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

stop_compositor() {
	local name pid session
	[[ $LEVEL == 2 ]] || return 0
	while read -r name; do
		pid=$(read_pid "$name.pid")
		[[ -n $pid ]] || pid=$(our_pid_named "$name" || true)
		session=$(read_pid "$name-session.pid")
		[[ -n $pid || -n $session ]] || continue
		note "stopping $name ${pid:-?}"
		kill_list TERM "$pid"
		wait_gone "${pid:-$session}" 10 || kill_list KILL "$pid"
		wait_gone "$session" 5 || kill_list KILL "$session"
		rm -f "$DIR/$name.pid" "$DIR/$name-session.pid"
	done < <(backend_processes)
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

supervisor_cleanup() {
	trap - EXIT TERM INT HUP USR1
	stop_daemon
	stop_compositor
	kill_stragglers
	[[ $LEVEL == 2 ]] && backend_keep_logs
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
	[[ $LEVEL == 2 ]] && start_compositor
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
	load_paths "${1:-}"
	stop_claude_server
	supervisor=$(read_pid supervisor.pid)
	if pid_alive "$supervisor"; then
		kill -TERM "$supervisor"
		wait_gone "$supervisor" 30 || die "the supervisor $supervisor did not stop"
	fi
	mapfile -t left < <(ours_pids)
	((${#left[@]} == 0)) || die "processes still carry this env: ${left[*]}"
	runtime_looks_ours "$RUNTIME" && rm -rf -- "$RUNTIME"
	remove_claude_projects
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
	backend_screenshot "$out"
	printf '%s\n' "$out"
}

cmd_ctl() {
	load_env "${1:-}"
	shift
	[[ $LEVEL == 2 ]] || die "ctl needs level 2"
	require_running
	assert_isolation
	backend_ctl "$@"
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

claude_server() {
	printf 'sfe-%s\n' "${RUNTIME##*/sfe.}"
}

env_tmux() {
	env -i "HOME=$HOME" "PATH=/usr/bin:/bin" "LANG=C.UTF-8" "TERM=xterm-256color" \
		tmux -L "$(claude_server)" "$@"
}

claude_name() {
	[[ ${1:-} =~ ^[a-z0-9-]+$ ]] || die "the session name '${1:-}' must match ^[a-z0-9-]+\$"
	printf '%s\n' "$1"
}

claude_id() {
	[[ -f $DIR/claude/$1.id ]] || die "this env started no Claude Code session named $1"
	cat "$DIR/claude/$1.id"
}

claude_alive() {
	env_tmux has-session -t "=$1" 2>/dev/null
}

claude_screen() {
	env_tmux capture-pane -p -J -t "=$1:" 2>/dev/null || true
}

press() {
	local name=$1 key
	shift
	for key in "$@"; do
		env_tmux send-keys -t "=$name:" "$key"
		sleep 0.3
	done
}

type_line() {
	refuse_permission "$1" && return 1
	env_tmux send-keys -t "=$1:" -l -- "$2"
	sleep 0.5
	refuse_permission "$1" && return 1
	env_tmux send-keys -t "=$1:" Enter
}

refuse_permission() {
	grep -qE "$CLAUDE_PERMISSION_PROMPT" <<<"$(claude_screen "$1")" || return 1
	note "session $1 shows a permission prompt; refusing it with Escape"
	press "$1" Escape
	sleep 1
}

clear_dialogs() {
	grep -q 'Teach auto mode' <<<"$(claude_screen "$1")" || return 0
	press "$1" Down Enter
	sleep 1
}

trust_pending() {
	grep -qi 'trust this folder' <<<"$(claude_screen "$1")"
}

claude_calls() {
	local id=$1 event=$2 file
	for file in "$DIR"/claude/calls/"$event"-*.in.json; do
		[[ -f $file ]] && grep -qF "\"session_id\":\"$id\"" "$file" && printf '%s\n' "$file"
	done
	return 0
}

hook_command() {
	printf 'bash %q hook %q %q' "$SCRIPT_PATH" "$DIR" "$1"
}

write_claude_settings() {
	local file=$DIR/claude/settings.json pair event arg entry hooks='{}'
	[[ -f $file ]] && return 0
	for pair in "${CLAUDE_HOOKS[@]}"; do
		read -r event arg <<<"$pair"
		entry=$(jq -n --arg command "$(hook_command "$arg")" \
			'{hooks: [{type: "command", command: $command, timeout: 5}]}')
		[[ $event == PreToolUse ]] && entry=$(jq --arg matcher "$CLAUDE_TOOL_MATCHER" '{matcher: $matcher} + .' <<<"$entry")
		hooks=$(jq --arg event "$event" --argjson entry "$entry" '.[$event] = [$entry]' <<<"$hooks")
	done
	jq -n --argjson hooks "$hooks" --argjson permissions "$(claude_permissions)" \
		'{permissions: $permissions, hooks: $hooks}' >"$file"
}

claude_permissions() {
	local tool allow=("${CLAUDE_ALLOWED_COMMANDS[@]}") deny=()
	for tool in Edit Write; do
		allow+=("$tool(/$DIR/**)")
	done
	for tool in "${CLAUDE_FILE_TOOLS[@]}"; do
		deny+=("$tool(~/.claude/**)" "$tool(~/.claude.json)" "$tool(/$HOME/.claude/**)" "$tool(/$HOME/.claude.json)")
	done
	jq -n --args '{defaultMode: "default", allow: $ARGS.positional[:$n], deny: $ARGS.positional[$n:]}' \
		--argjson n "${#allow[@]}" \
		"${allow[@]}" "${deny[@]}"
}

claude_ready() {
	[[ -n $(claude_calls "$2" session-start) ]] && ! trust_pending "$1"
}

cmd_claude_start() {
	local name cwd claude id start deadline trusted=0
	load_env "${1:-}"
	name=$(claude_name "${2:-}")
	cwd=$(readlink -f "${3:?claude-start needs a working directory}")
	[[ -d $cwd && $cwd == "$DIR"/* ]] || die "the working directory $cwd must be a folder inside $DIR"
	claude=$(command -v claude) || die "claude is not on PATH"
	require_running
	assert_isolation
	mkdir -p "$DIR/claude/calls"
	[[ ! -f $DIR/claude/$name.id ]] || die "this env already started a session named $name"
	write_claude_settings
	id=$(</proc/sys/kernel/random/uuid)
	printf '%s\n' "$id" >"$DIR/claude/$name.id"
	env_tmux new-session -d -s "$name" -x 200 -y 50 -c "$cwd" -- env -i "HOME=$HOME" "USER=$(id -un)" \
		"PATH=/usr/bin:/bin" "SHELL=/usr/bin/bash" "LANG=C.UTF-8" "TERM=xterm-256color" \
		"XDG_RUNTIME_DIR=$RUNTIME" "$claude" --setting-sources project --strict-mcp-config \
		--settings "$DIR/claude/settings.json" --session-id "$id" --model haiku
	deadline=$((SECONDS + CLAUDE_START_TIMEOUT))
	until claude_ready "$name" "$id"; do
		claude_alive "$name" || die "session $name exited at start"
		((SECONDS < deadline)) || die "session $name was not ready in ${CLAUDE_START_TIMEOUT}s: $(claude_screen "$name")"
		if ((!trusted)) && trust_pending "$name"; then
			trusted=1
			press "$name" Down Enter
		fi
		sleep 0.5
	done
	sleep 2
	start=$(claude_calls "$id" session-start | tail -n 1)
	printf 'session %s id %s, SessionStart env: %s\n' "$name" "$id" "$(paste -sd ' ' "${start%.in.json}.env")" >&2
}

cmd_claude_send() {
	local name
	load_env "${1:-}"
	name=$(claude_name "${2:-}")
	claude_alive "$name" || die "session $name is not running"
	clear_dialogs "$name"
	type_line "$name" "${3:?claude-send needs a text}" ||
		die "session $name showed a permission prompt; it was refused and the text was not sent"
}

cmd_claude_deny() {
	local name
	load_env "${1:-}"
	name=$(claude_name "${2:-}")
	claude_alive "$name" || return 0
	refuse_permission "$name" || true
}

cmd_claude_pane() {
	local name
	load_env "${1:-}"
	name=$(claude_name "${2:-}")
	claude_alive "$name" || die "session $name is not running"
	env_tmux capture-pane -p -J -t "=$name:" -S -
}

cmd_claude_stop() {
	local name deadline
	load_env "${1:-}"
	name=$(claude_name "${2:-}")
	claude_alive "$name" || return 0
	clear_dialogs "$name"
	type_line "$name" /exit || type_line "$name" /exit || note "session $name keeps showing a permission prompt"
	deadline=$((SECONDS + CLAUDE_STOP_TIMEOUT))
	while claude_alive "$name"; do
		if ((SECONDS >= deadline)); then
			note "session $name did not exit in ${CLAUDE_STOP_TIMEOUT}s; killing it"
			env_tmux kill-session -t "=$name"
			return 0
		fi
		sleep 0.5
	done
}

cmd_claude_id() {
	load_paths "${1:-}"
	claude_id "$(claude_name "${2:-}")"
}

cmd_claude_calls() {
	local name
	load_paths "${1:-}"
	name=$(claude_name "${2:-}")
	claude_calls "$(claude_id "$name")" "${3:?claude-calls needs an event}"
}

cmd_hook() {
	local event base name vars=()
	load_env "${1:-}"
	event=${2:?hook needs an event}
	[[ $event =~ ^[a-z-]+$ ]] || die "the event '$event' must match ^[a-z-]+\$"
	require_running
	assert_isolation
	mkdir -p "$DIR/claude/calls"
	base=$DIR/claude/calls/$event-$(date +%s%N)
	cat >"$base.in.json"
	for name in CLAUDE_CODE_SESSION_ATTENDED CLAUDE_CODE_ENTRYPOINT; do
		[[ -v $name ]] && vars+=("$name=${!name}")
	done
	[[ ${CLAUDE_CODE_MESSAGING_SOCKET:-} == "$RUNTIME"/* ]] &&
		vars+=("CLAUDE_CODE_MESSAGING_SOCKET=$CLAUDE_CODE_MESSAGING_SOCKET")
	printf '%s\n' "${vars[@]}" >"$base.env"
	env -i "${ISO_ENV[@]}" "${vars[@]}" "${SANDBOX[@]}" selflow claude hook "$event" \
		<"$base.in.json" >"$base.out.part"
	mv "$base.out.part" "$base.out.json"
	cat "$base.out.json"
}

measure_script() {
	cat <<'EOF'
count=$1 work=$2
for kind in prompt tool; do
	selflow claude hook "$kind" <"$work/$kind.json" >"$work/$kind.first"
	for ((i = 0; i < count; i++)); do
		start=$EPOCHREALTIME
		selflow claude hook "$kind" <"$work/$kind.json" >/dev/null
		end=$EPOCHREALTIME
		printf '%s %s %s\n' "$kind" "$start" "$end"
	done
	selflow claude hook "$kind" <"$work/$kind.json" >"$work/$kind.out"
done
EOF
}

cmd_measure_hook() {
	local count cwd work kind fixtures=$REPO_ROOT/crates/selflow/tests/fixtures/claude
	load_env "${1:-}"
	count=${2:-100}
	cwd=$(readlink -f "${3:-$DIR/home}")
	[[ $count =~ ^[1-9][0-9]*$ ]] || die "N must be a positive number"
	require_running
	assert_isolation
	work=$DIR/claude/measure
	mkdir -p "$work"
	jq -c --arg cwd "$cwd" '.cwd = $cwd' "$fixtures/prompt.json" >"$work/prompt.json"
	jq -c --arg cwd "$cwd" '.cwd = $cwd' "$fixtures/tool-bash.json" >"$work/tool.json"
	env -i "${ISO_ENV[@]}" CLAUDE_CODE_SESSION_ATTENDED=1 CLAUDE_CODE_ENTRYPOINT=cli "${SANDBOX[@]}" \
		bash -c "$(measure_script)" measure "$count" "$work" >"$work/times.txt"
	for kind in prompt tool; do
		printf '%s first output: %s\n%s output: %s\n' "$kind" "$(<"$work/$kind.first")" "$kind" "$(<"$work/$kind.out")"
	done
	for kind in prompt tool; do
		(($(awk -v kind="$kind" '$1 == kind' "$work/times.txt" | wc -l) == count)) ||
			die "measure-hook recorded fewer than $count $kind calls (see $work/times.txt)"
	done
	for kind in prompt tool; do
		awk -v kind="$kind" '$1 == kind { printf "%.3f\n", ($3 - $2) * 1000 }' "$work/times.txt" | sort -n |
			awk -v kind="$kind" '{ v[NR] = $1 } END {
				printf "%s: n %d p50 %.2f ms p95 %.2f ms max %.2f ms\n", kind, NR,
					v[int((NR * 50 + 99) / 100)], v[int((NR * 95 + 99) / 100)], v[NR]
			}'
	done
}

stop_claude_server() {
	local waited=0
	[[ -d $DIR/claude ]] || return 0
	env_tmux kill-server 2>/dev/null || true
	while our_pid_named claude >/dev/null && ((waited < 100)); do
		sleep 0.1
		waited=$((waited + 1))
	done
}

remove_claude_projects() {
	local encoded folder
	encoded=$(basename "$DIR" | tr -c 'A-Za-z0-9\n' '-')
	[[ $encoded =~ ^selflow-iso-[A-Za-z0-9]{6}$ ]] || return 0
	for folder in "$HOME"/.claude/projects/*"$encoded"*; do
		[[ -d $folder && ! -L $folder ]] || continue
		rm -rf -- "$folder"
		printf 'removed %s\n' "$folder" >&2
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
		["ctl"]=cmd_ctl
		["hyprctl"]=cmd_ctl
		["measure-idle"]=cmd_measure_idle
		["env"]=cmd_env
		["claude-start"]=cmd_claude_start
		["claude-send"]=cmd_claude_send
		["claude-pane"]=cmd_claude_pane
		["claude-deny"]=cmd_claude_deny
		["claude-stop"]=cmd_claude_stop
		["claude-id"]=cmd_claude_id
		["claude-calls"]=cmd_claude_calls
		["hook"]=cmd_hook
		["measure-hook"]=cmd_measure_hook
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
