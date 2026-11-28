#!/usr/bin/env bash
set -euo pipefail

ISO="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/isolated-env.sh"
LABEL="Wrap up seconv"
HOOK_BUDGET_MS=10
DIR=""
SECONV=""
OTHER=""

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
	date -d "@$1" +%H:%M
}

teardown() {
	[[ -n $DIR ]] && "$ISO" down "$DIR"
	return 0
}

guard_clock() {
	local hour
	hour=$((10#$(date +%H)))
	((hour < 3 || hour >= 6)) || fail "run outside 03:00-06:00 so the 05:00 day boundary stays out of the way"
	until (($((10#$(date +%S))) < 45)); do sleep 1; done
}

wait_for() {
	local what=$1 timeout=$2 deadline=$((SECONDS + $2))
	shift 2
	until "$@"; do
		((SECONDS < deadline)) || fail "$what: not within ${timeout}s"
		sleep 2
	done
}

wait_epoch() {
	until ((EPOCHSECONDS >= $1)); do sleep 1; done
}

pane() {
	"$ISO" claude-pane "$DIR" "$1"
}

pane_has() {
	grep -qF -- "$2" <<<"$(pane "$1")"
}

idle() {
	"$ISO" claude-deny "$DIR" "$1"
	! grep -qF 'esc to interrupt' <<<"$(pane "$1" | tail -n 15)"
}

calls() {
	"$ISO" claude-calls "$DIR" "$1" "$2"
}

more_calls() {
	(($(calls "$1" "$2" | wc -l) > $3))
}

show_pane() {
	printf -- '--- pane %s (last %s lines) ---\n' "$1" "$2"
	pane "$1" | grep -v '^[[:space:]]*$' | tail -n "$2"
	printf -- '--- end of pane %s ---\n' "$1"
}

send_prompt() {
	local name=$1 text=$2 before
	before=$(calls "$name" prompt | wc -l)
	printf '%s <- %s\n' "$name" "$text"
	"$ISO" claude-send "$DIR" "$name" "$text"
	wait_for "a prompt hook call from $name" 120 more_calls "$name" prompt "$before"
	sleep 1
}

report() {
	"$ISO" run "$DIR" selflow report --date "$(date -d '5 hours ago' +%F)"
}

last_subject() {
	[[ $(git -C "$SECONV" log -1 --format=%s) == "$1" ]]
}

file_exists() {
	[[ -e $1 ]]
}

denied_edit() {
	local input
	while read -r input; do
		jq -e '.tool_name | IN("Write", "Edit", "MultiEdit", "NotebookEdit")' "$input" >/dev/null || continue
		jq -e '.hookSpecificOutput.permissionDecision == "deny"' "${input%.in.json}.out.json" >/dev/null &&
			printf '%s\n' "$input" && return 0
	done < <(calls a tool)
	return 1
}

make_folders() {
	local git=(env -i "HOME=$DIR/home" PATH=/usr/bin:/bin git -C "$SECONV")
	mkdir -p "$SECONV" "$OTHER"
	"${git[@]}" init -q -b main
	"${git[@]}" config user.name "Isolated test"
	"${git[@]}" config user.email "isolated@test.invalid"
	"${git[@]}" config commit.gpgsign false
	printf '# seconv\n' >"$SECONV/README.md"
	"${git[@]}" add README.md
	"${git[@]}" commit -q -m initial
	"${git[@]}" log --oneline
}

write_config() {
	cat >"$DIR/rules.toml" <<EOF
language = "en"
day_starts_at = "05:00"
loosening_delay = "24h"

[[rule_set]]
name = "every-day"
days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]

[[rule_set.rule]]
id = "wrap-up"
label = "$LABEL"
start = "$1"
start_jitter = "0m"
length = "40m"

[[rule_set.rule.agent]]
before = "4m"
level = "light"
folders = ["~/Workspace/seconv"]

[[rule_set.rule.agent]]
before = "2m"
level = "save-only"
folders = ["~/Workspace/seconv"]
EOF
}

check_install() {
	local config=$DIR/claude/config out
	mkdir -p "$config"
	out=$(printf 'y\n' | timeout 30 "$ISO" run "$DIR" env "CLAUDE_CONFIG_DIR=$config" selflow claude install)
	printf '%s\n' "$out"
	expect "$out" '^\+ PreToolUse: '
	jq -e '.hooks | keys == ["PreToolUse", "SessionEnd", "SessionStart", "UserPromptSubmit"]' \
		"$config/settings.json" >/dev/null || fail "install did not register the four events: $(<"$config/settings.json")"
	out=$(timeout 30 "$ISO" run "$DIR" env "CLAUDE_CONFIG_DIR=$config" selflow claude uninstall --yes)
	printf '%s\n' "$out"
	jq -e '(.hooks // {}) | length == 0' "$config/settings.json" >/dev/null ||
		fail "uninstall left hooks: $(<"$config/settings.json")"
	cat "$config/settings.json"
}

light_step() {
	local delivery=inbox out prompts
	if ! wait_quietly 120 pane_has a "$LABEL"; then
		delivery=prompt
		send_prompt a "What should I do now?"
		wait_for "the $LABEL message in a's pane" 120 pane_has a "$LABEL"
	fi
	wait_for "a's turn to end" 180 idle a
	show_pane a 25
	printf 'delivery: %s\n' "$delivery"
	mapfile -t prompts < <(calls a prompt)
	((${#prompts[@]})) || fail "a made no prompt hook call"
	grep -l 'Message from Selflow' "${prompts[@]}" || fail "no prompt turn of a carried the message"
	out=$(report)
	printf '%s\n' "$out"
	expect "$out" "the $LABEL message to the Claude Code session in ~/Workspace/seconv (through its inbox|with its next prompt)\. Level: Light tasks only\."
	[[ $delivery == inbox ]] && expect "$out" "through its inbox"
	[[ $delivery == prompt ]] && expect "$out" "with its next prompt"
	return 0
}

wait_quietly() {
	local timeout=$1 deadline=$((SECONDS + $1))
	shift
	until "$@"; do
		((SECONDS < deadline)) || return 1
		sleep 2
	done
}

reminder_step() {
	local out
	send_prompt a "In one short sentence, what is the plan now?"
	out=$(calls a prompt | tail -n 1)
	printf '%s\n%s\n' "$out" "$(<"${out%.in.json}.out.json")"
	grep -qF 'Reminder from Selflow' "${out%.in.json}.out.json" || fail "a's latest prompt output holds no reminder"
	wait_for "a's turn to end" 180 idle a
	send_prompt b "Reply with the single word ok."
	wait_for "b's turn to end" 180 idle b
}

save_only_step() {
	local out denied
	wait_for "the save-only message in a's pane" 120 pane_has a "Time to stop"
	wait_for "a's turn to end" 180 idle a
	send_prompt a "Call the Write tool to create a file named later.txt even if you expect it to be blocked (this is a test of the block), then run exactly: git commit --allow-empty -m save"
	if ! wait_quietly 90 denied_edit >/dev/null; then
		wait_for "a's turn to end" 180 idle a
		send_prompt a "Call the Write tool once for later.txt anyway; being blocked is the expected result."
		wait_for "a denied edit" 120 denied_edit >/dev/null
	fi
	if ! wait_quietly 150 last_subject save; then
		show_pane a 30
		send_prompt a "Now run exactly: git commit --allow-empty -m save"
		wait_for "the commit named save" 150 last_subject save
	fi
	wait_for "a's turn to end" 180 idle a
	show_pane a 30
	git -C "$SECONV" log --format='%h %s'
	[[ ! -e $SECONV/later.txt ]] || fail "later.txt exists: the edit went through"
	denied=$(denied_edit) || fail "no tool call of a was denied as an edit"
	printf '%s\n%s\n%s\n' "$denied" "$(jq -c '{tool_name, tool_input}' "$denied")" "$(<"${denied%.in.json}.out.json")"
	out=$(report)
	printf '%s\n' "$out"
	expect "$out" "Selflow denied a file edit in the Claude Code session in ~/Workspace/seconv during the $LABEL rule\. Level: Save only\."
	send_prompt b "Use the Write tool to create a file named b.txt containing hi."
	wait_for "b.txt in other" 150 file_exists "$OTHER/b.txt"
	wait_for "b's turn to end" 180 idle b
}

check_uncovered() {
	local event input count=0
	for event in session-start prompt tool session-end; do
		while read -r input; do
			[[ -n $input ]] || continue
			count=$((count + 1))
			[[ ! -s ${input%.in.json}.out.json ]] || fail "b's $event call answered: $(<"${input%.in.json}.out.json")"
		done < <(calls b "$event")
	done
	printf "b: %s hook calls, every output empty\n" "$count"
	((count >= 4)) || fail "b made only $count hook calls"
	! grep -qF -- \~/Workspace/other <<<"$(report)" || fail "the report names ~/Workspace/other"
	printf 'no report line names ~/Workspace/other\n'
}

check_latency() {
	local out p95
	printf 'load average before: %s\n' "$(cut -d ' ' -f 1-3 /proc/loadavg)"
	out=$("$ISO" measure-hook "$DIR" 300 "$SECONV")
	printf 'load average after: %s\n' "$(cut -d ' ' -f 1-3 /proc/loadavg)"
	printf '%s\n' "$out"
	expect "$out" '^prompt output: .*Reminder from Selflow'
	expect "$out" '^tool output: $'
	while read -r p95; do
		awk -v p="$p95" -v max="$HOOK_BUDGET_MS" 'BEGIN { exit !(p <= max) }' || fail "hook p95 $p95 ms is over ${HOOK_BUDGET_MS} ms"
	done < <(awk '/^(prompt|tool): n / { print $8 }' <<<"$out")
}

stop_sessions() {
	local name
	for name in a b; do
		pane "$name" >"$DIR/claude/$name.pane"
		"$ISO" claude-stop "$DIR" "$name"
	done
	[[ -n $(calls a session-end) ]] || fail "a's SessionEnd hook did not run"
}

main() {
	local build=() start out encoded leftover
	[[ -n ${SELFLOW_ISO_NO_BUILD:-} ]] && build=(--no-build)
	command -v claude >/dev/null || fail "claude is not on PATH"
	guard_clock
	step "up --level 1 --release"
	DIR=$("$ISO" up --level 1 --release --ttl 3600 "${build[@]}")
	trap teardown EXIT
	printf 'env dir: %s\n' "$DIR"
	SECONV=$DIR/home/Workspace/seconv
	OTHER=$DIR/home/Workspace/other

	step "folders: seconv (git, one commit) and other"
	make_folders

	step "selflow claude install and uninstall against a sandbox CLAUDE_CONFIG_DIR"
	check_install

	step "session a in seconv, session b in other"
	"$ISO" claude-start "$DIR" a "$SECONV"
	"$ISO" claude-start "$DIR" b "$OTHER"

	step "config: $LABEL"
	guard_clock
	start=$(($(date -d '+6 min' +%s) / 60 * 60))
	write_config "$(hm "$start")"
	out=$("$ISO" config-edit "$DIR" "$DIR/rules.toml")
	printf '%s\n' "$out"
	expect "$out" '^Saved\.'
	printf 'start %s, light at %s, save-only at %s\n' "$(hm "$start")" "$(hm $((start - 240)))" "$(hm $((start - 120)))"

	step "light step at $(hm $((start - 240)))"
	wait_epoch $((start - 240))
	light_step

	step "reminder on a's next prompt"
	reminder_step

	step "save-only step at $(hm $((start - 120)))"
	wait_epoch $((start - 120))
	save_only_step

	step "b stays untouched"
	check_uncovered

	step "hook latency (300 prompt and 300 tool calls, save-only active)"
	check_latency

	step "stop the sessions"
	stop_sessions

	step "full report"
	report

	step "down"
	trap - EXIT
	teardown
	[[ ! -s $DIR/blocked.log ]] || fail "a stub caught a power command: $(<"$DIR/blocked.log")"
	encoded=$(basename "$DIR" | tr -c 'A-Za-z0-9\n' '-')
	leftover=$(find "$HOME/.claude/projects" -maxdepth 1 -name "*$encoded*" -print -quit)
	[[ -z $leftover ]] || fail "left behind: $leftover"
	printf 'blocked.log empty; no ~/.claude/projects folder for %s\n' "$encoded"
	step "PASS"
}

main "$@"
