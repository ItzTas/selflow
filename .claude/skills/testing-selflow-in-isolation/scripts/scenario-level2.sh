#!/usr/bin/env bash
set -euo pipefail

ISO="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/isolated-env.sh"
DIR=""

step() {
	printf '\n== [%(%H:%M:%S)T] %s\n' -1 "$*"
}

fail() {
	printf 'FAIL: %s\n' "$*" >&2
	exit 1
}

expect() {
	grep -qE -- "$2" <<<"$1" || fail "expected /$2/ in: $1"
}

hm() {
	date -d "$1" +%H:%M
}

teardown() {
	[[ -n $DIR ]] && "$ISO" down "$DIR"
	return 0
}

write_config() {
	cat >"$DIR/lock.toml" <<EOF
language = "en"
day_starts_at = "05:00"
loosening_delay = "24h"

[anchors]
bedtime = "$(hm '+90 min')"
wake = "$(hm '+150 min')"

[[rule_set]]
name = "every-night"
days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]

[rule_set.challenges]
wait = "1m"
reason = true
phrase = ""
extensions = 3
extension_length = "20m"

[[rule_set.rule]]
id = "bedtime"
label = "Bedtime"
start = "bedtime"
start_jitter = "0m"
end = "wake"
lock = true
EOF
}

client_pid() {
	local deadline=$((SECONDS + 20)) pid
	while ((SECONDS < deadline)); do
		pid=$("$ISO" lock-pids "$DIR" | awk 'NR == 1 { print $1 }')
		[[ -n $pid ]] && printf '%s\n' "$pid" && return 0
		sleep 1
	done
	fail "no selflow-ui client appeared"
}

start_test_lock() {
	local deadline=$((SECONDS + 240)) out
	while ((SECONDS < deadline)); do
		out=$(timeout 20 "$ISO" run "$DIR" selflow test-lock --for 3m 2>&1 || true)
		printf '%s\n' "$out"
		grep -q '^Test lock' <<<"$out" && return 0
		sleep 10
	done
	fail "test-lock was refused for 240s"
}

unlock() {
	printf '%s\n' "$1" | timeout 150 "$ISO" run "$DIR" selflow unlock
}

assert_no_client() {
	[[ -z $("$ISO" lock-pids "$DIR") ]] || fail "a lock client is still running: $("$ISO" lock-pids "$DIR")"
}

main() {
	local out pid from
	local build=()
	[[ -n ${SELFLOW_ISO_NO_BUILD:-} ]] && build=(--no-build)
	step "up --level 2 (headless mutter host + nested Hyprland, both invisible)"
	DIR=$("$ISO" up --level 2 --ttl 1800 "${build[@]}")
	trap teardown EXIT
	printf 'env dir: %s\n' "$DIR"
	"$ISO" ctl "$DIR" monitors | sed -n 1,2p
	"$ISO" hyprctl "$DIR" version | sed -n 1p

	step "save a config with a lock rule"
	write_config
	"$ISO" config-edit "$DIR" "$DIR/lock.toml"

	step "test-lock --for 3m"
	start_test_lock
	pid=$(client_pid)
	sleep 3
	"$ISO" lock-pids "$DIR"
	"$ISO" screenshot "$DIR" "$DIR/screenshots/test-lock.png"

	step "unlock with a piped reason (read after the 1m wait)"
	out=$(unlock "Checking the unlock flow in the isolated harness.")
	printf '%s\n' "$out"
	expect "$out" '^> Unlocked until|Unlocked until'
	sleep 3
	assert_no_client
	"$ISO" screenshot "$DIR" "$DIR/screenshots/after-unlock.png"

	step "second test lock (waits until the first one's extension ends)"
	start_test_lock
	pid=$(client_pid)
	sleep 3
	"$ISO" screenshot "$DIR" "$DIR/screenshots/test-lock-2.png"

	step "freeze the client (kill -STOP $pid), then unlock"
	from=$("$ISO" log-lines "$DIR")
	kill -STOP "$pid"
	"$ISO" lock-pids "$DIR"
	out=$(unlock "Freezing the client to check the release path.")
	printf '%s\n' "$out"
	expect "$out" 'Unlocked until'
	"$ISO" wait-log "$DIR" 'starting a client to release the screen' 30 "$from"
	"$ISO" wait-log "$DIR" 'the release client exited' 30 "$from"
	tail -n +"$((from + 1))" "$DIR/daemon.log" | grep -v wgpu
	sleep 2
	assert_no_client
	"$ISO" screenshot "$DIR" "$DIR/screenshots/after-release.png"

	step "idle cost"
	"$ISO" measure-idle "$DIR" 10

	step "blocked power commands"
	[[ ! -s $DIR/blocked.log ]] || fail "a stub caught a power command: $(<"$DIR/blocked.log")"
	printf 'none\n'
	step "PASS"
}

main "$@"
