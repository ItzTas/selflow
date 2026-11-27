#!/usr/bin/env bash
set -euo pipefail

ISO="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/isolated-env.sh"
REASON="Testing the exception flow in the isolated harness today."
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

wait_notice() {
	local text=$1 timeout=$2 deadline=$((SECONDS + $2))
	until grep -qF -- "$text" "$DIR/notifications.log"; do
		((SECONDS < deadline)) || fail "no notification with '$text' in ${timeout}s"
		sleep 1
	done
}

guard_clock() {
	local hour
	hour=$((10#$(date +%H)))
	((hour < 3 || hour >= 6)) || fail "run outside 03:00-06:00 so the 05:00 day boundary stays out of the way"
	until (($((10#$(date +%S))) < 45)); do sleep 1; done
}

write_first_config() {
	cat >"$DIR/config1.toml" <<EOF
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
extensions = 1
extension_length = "20m"

[[rule_set.rule]]
id = "bedtime"
label = "Bedtime"
start = "bedtime"
start_jitter = "0m"
end = "wake"
notices = ["15m", "5m", "1m"]
lock = true

[[rule_set.rule]]
id = "stretch"
start = "$(hm '+2 min')"
message = "Isolated reminder: stretch."

[[rule_set.rule]]
id = "poweroff-test"
label = "Power-off test"
start = "$(hm '+3 min')"
end = "$(hm '+30 min')"
notices = ["1m"]
shutdown = "once"
EOF
}

write_second_config() {
	local old new
	old=$(sed -n 's/^bedtime = "\(.*\)"$/\1/p' "$DIR/config1.toml")
	new=$(date -d "$old 10 min ago" +%H:%M)
	sed -e "s/^bedtime = \".*\"/bedtime = \"$new\"/" -e 's/^extensions = 1$/extensions = 2/' \
		"$DIR/config1.toml" >"$DIR/config2.toml"
}

main() {
	local out from shutdown_line logical_date
	local build=()
	[[ -n ${SELFLOW_ISO_NO_BUILD:-} ]] && build=(--no-build)
	guard_clock
	step "up --level 1"
	DIR=$("$ISO" up --level 1 --ttl 1800 "${build[@]}")
	trap teardown EXIT
	printf 'env dir: %s\n' "$DIR"

	step "first config edit"
	write_first_config
	out=$("$ISO" config-edit "$DIR" "$DIR/config1.toml")
	printf '%s\n' "$out"
	expect "$out" '^Saved\.'
	expect "$out" '^Applies now:'

	step "reminder notice (stretch, start $(sed -n '/id = "stretch"/{n;p}' "$DIR/config1.toml"))"
	wait_notice "Isolated reminder: stretch." 200
	grep -B4 -A1 'Isolated reminder' "$DIR/notifications.log" | grep -E 'method call|string "'
	"$ISO" wait-log "$DIR" "couldn't show a notification" 5

	step "lockless shutdown = once"
	shutdown_line=$("$ISO" wait-log "$DIR" 'shutdown requested' 200)
	printf '%s\n(rule start %s)\n' "$shutdown_line" "$(sed -n '/id = "poweroff-test"/{n;n;p}' "$DIR/config1.toml")"

	step "restart the daemon; no second shutdown within 80s"
	from=$("$ISO" log-lines "$DIR")
	"$ISO" restart-daemon "$DIR"
	"$ISO" wait-log "$DIR" 'shutdown requested' 80 "$from" && fail "a second shutdown was requested after the restart"
	tail -n +"$((from + 1))" "$DIR/daemon.log"

	step "second save: earlier start + extensions = 2"
	write_second_config
	out=$("$ISO" config-edit "$DIR" "$DIR/config2.toml")
	printf '%s\n' "$out"
	expect "$out" '^Applies now:'
	expect "$out" '^Waits until '

	step "config status"
	out=$("$ISO" run "$DIR" selflow config status)
	printf '%s\n' "$out"
	expect "$out" 'Waits until '

	step "exception --rule bedtime (reason piped, read after the wait)"
	out=$(printf '%s\n' "$REASON" | timeout 150 "$ISO" run "$DIR" selflow exception --rule bedtime)
	printf '%s\n' "$out"
	expect "$out" 'is off for the rest of the day'

	step "report"
	logical_date=$(date -d '5 hours ago' +%F)
	out=$("$ISO" run "$DIR" selflow report --date "$logical_date")
	printf '%s\n' "$out"
	expect "$out" 'You skipped the Bedtime rule'

	step "idle cost"
	"$ISO" measure-idle "$DIR" 10

	step "blocked power commands"
	[[ ! -s $DIR/blocked.log ]] || fail "a stub caught a power command: $(<"$DIR/blocked.log")"
	printf 'none\n'
	step "PASS"
}

main "$@"
