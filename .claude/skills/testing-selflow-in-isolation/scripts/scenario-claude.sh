#!/usr/bin/env bash
set -euo pipefail

ISO="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/isolated-env.sh"
LABEL="Wrap up seconv"
HOOK_BUDGET_MS=10
HOOK_CALLS=300
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
	local offset hour
	for offset in 0 50; do
		hour=$((10#$(date -d "+$offset min" +%H)))
		((hour < 3 || hour >= 6)) ||
			fail "run so that now and the next 50 minutes stay outside 03:00-06:00 (the 05:00 day boundary)"
	done
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
	local name=$1 text=$2 before latest
	before=$(calls "$name" prompt | wc -l)
	printf '%s <- %s\n' "$name" "$text"
	"$ISO" claude-send "$DIR" "$name" "$text"
	wait_for "a prompt hook call from $name" 120 more_calls "$name" prompt "$before"
	latest=$(calls "$name" prompt | tail -n 1)
	wait_for "the output of $name's prompt hook" 30 file_exists "${latest%.in.json}.out.json"
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

denied_call() {
	local input
	while read -r input; do
		[[ -f ${input%.in.json}.out.json ]] || continue
		jq -e "$1" "$input" >/dev/null || continue
		jq -e '.hookSpecificOutput.permissionDecision == "deny"' "${input%.in.json}.out.json" >/dev/null &&
			printf '%s\n' "$input" && return 0
	done < <(calls a tool)
	return 1
}

denied_edit() {
	denied_call '.tool_name | IN("Write", "Edit", "MultiEdit", "NotebookEdit")'
}

denied_chain() {
	denied_call '.tool_name == "Bash" and (.tool_input.command | contains("&&"))'
}

commit_saved() {
	last_subject save
}

show_call() {
	printf '%s\n%s\n%s\n' "$1" "$(jq -c '{tool_name, tool_input}' "$1")" "$(<"${1%.in.json}.out.json")"
}

ensure() {
	local what=$1 check=$2 fallback=$3
	wait_quietly 90 "$check" >/dev/null && return 0
	wait_for "a's turn to end" 180 idle a
	show_pane a 30
	send_prompt a "$fallback"
	wait_for "$what" 120 "$check" >/dev/null
}

absent_before() {
	wait_epoch $(($2 - 5))
	! pane_has a "$1" || fail "a's pane shows '$1' before $(hm "$2")"
	printf "a's pane has no '%s' at %(%H:%M:%S)T\n" "$1" -1
	wait_epoch "$2"
}

message_line() {
	printf '^%s +Selflow (sent|gave) the %s message to the Claude Code session in ~/Workspace/seconv (through its inbox|with its next prompt)\\. Level: %s\\.' \
		"$(hm "$1")" "$LABEL" "$2"
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
	local at=$1 delivery=inbox out prompts carrier
	absent_before "$LABEL" "$at"
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
	[[ $delivery == prompt ]] && prompts=("${prompts[@]/%.in.json/.out.json}")
	carrier=$(grep -l 'Message from Selflow' "${prompts[@]}") || fail "no prompt turn of a carried the message"
	printf 'message carried by %s\n' "$carrier"
	out=$(report)
	printf '%s\n' "$out"
	expect "$out" "$(message_line "$at" "Light tasks only")"
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
	wait_for "a's reminder" 30 test -s "${out%.in.json}.out.json"
	printf '%s\n%s\n' "$out" "$(<"${out%.in.json}.out.json")"
	grep -qF 'Reminder from Selflow' "${out%.in.json}.out.json" || fail "a's latest prompt output holds no reminder"
	wait_for "a's turn to end" 180 idle a
	send_prompt b "Reply with the single word ok."
	wait_for "b's turn to end" 180 idle b
}

save_only_step() {
	local at=$1 out
	absent_before "Time to stop" "$at"
	wait_for "the save-only message in a's pane" 120 pane_has a "Time to stop"
	wait_for "a's turn to end" 180 idle a
	send_prompt a "This is a test of Selflow's block, and being blocked is the expected result. Make three separate tool calls, one at a time, and go on after a block: first the Write tool, creating a file named later.txt; then the Bash command exactly: git status && touch f; then the Bash command exactly: git commit --allow-empty -m save"
	ensure "a denied edit" denied_edit "Call the Write tool once for later.txt anyway; being blocked is the expected result."
	ensure "a denied chained command" denied_chain "Run the Bash command exactly: git status && touch f (being blocked is the expected result)."
	ensure "the commit named save" commit_saved "Now run exactly: git commit --allow-empty -m save"
	wait_for "a's turn to end" 180 idle a
	show_pane a 30
	git -C "$SECONV" log --format='%h %s'
	[[ ! -e $SECONV/later.txt ]] || fail "later.txt exists: the edit went through"
	[[ ! -e $SECONV/f ]] || fail "f exists: the chained command went through"
	printf 'later.txt and f are absent\n'
	show_call "$(denied_edit)"
	show_call "$(denied_chain)"
	out=$(report)
	printf '%s\n' "$out"
	expect "$out" "$(message_line "$at" "Save only")"
	expect "$out" "Selflow denied a file edit in the Claude Code session in ~/Workspace/seconv during the $LABEL rule\. Level: Save only\."
	expect "$out" "Selflow denied a shell command in the Claude Code session in ~/Workspace/seconv during the $LABEL rule\. Level: Save only\."
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
	! pane_has b "$LABEL" || fail "b's pane shows $LABEL"
	! pane_has b "Message from Selflow" || fail "b's pane shows a Selflow message"
	printf "b's pane shows neither %s nor a Selflow message\n" "$LABEL"
	! grep -qF -- \~/Workspace/other <<<"$(report)" || fail "the report names ~/Workspace/other"
	printf 'no report line names ~/Workspace/other\n'
}

check_latency() {
	local run out kind n p95 lines over=0
	for run in 1 2 3; do
		printf -- '-- measurement %s, load average before: %s\n' "$run" "$(cut -d ' ' -f 1-3 /proc/loadavg)"
		out=$("$ISO" measure-hook "$DIR" "$HOOK_CALLS" "$SECONV")
		printf '%s\nload average after: %s\n' "$out" "$(cut -d ' ' -f 1-3 /proc/loadavg)"
		expect "$out" '^prompt output: .*Reminder from Selflow'
		expect "$out" '^tool output: $'
		lines=0
		while read -r kind n p95; do
			lines=$((lines + 1))
			((n == HOOK_CALLS)) || fail "measurement $run counted $n $kind calls, not $HOOK_CALLS"
			awk -v p="$p95" -v max="$HOOK_BUDGET_MS" 'BEGIN { exit !(p != "" && p <= max) }' && continue
			printf 'measurement %s: %s p95 %s ms is over %s ms\n' "$run" "$kind" "$p95" "$HOOK_BUDGET_MS"
			over=1
		done < <(awk '/^(prompt|tool): n / { print $1, $3, $8 }' <<<"$out")
		((lines == 2)) || fail "measurement $run printed $lines stat lines, not 2"
	done
	((over == 0)) || fail "hook p95 over ${HOOK_BUDGET_MS} ms in at least one of three measurements"
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
	light_step $((start - 240))

	step "reminder on a's next prompt"
	reminder_step

	step "save-only step at $(hm $((start - 120)))"
	save_only_step $((start - 120))

	step "b stays untouched"
	check_uncovered

	step "hook latency (three runs of $HOOK_CALLS prompt and $HOOK_CALLS tool calls, save-only active)"
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
