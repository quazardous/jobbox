#!/usr/bin/env bash
# A FAKE AGENT, DRIVING A REAL HOOK.
#
# The chrome is theatre: the prompt, the thinking dots, the tool call
# banner. Everything below the banner is not — the payload is the one a
# harness really sends, it goes through the installed `jbx hook`, and the
# line that runs is the line the hook wrote back. The detachment message,
# the id, the exit code and the ending are all produced by the binary.
#
# THAT SPLIT IS THE POINT. A screen recording of a real session drifts
# from the product without anyone noticing; this cannot, because the only
# thing it fakes is the part that is not being demonstrated.
#
# TIME IS COMPRESSED BY A SETTING, NOT BY A CUT. `JBX_AFTER` is the
# threshold; three seconds makes the detachment happen on camera. Nothing
# about the behaviour is faked — the cut is simply nearer, and the demo
# says so on screen.
set -u

JBX=${JBX:-$(command -v jbx)}
export JBX_AFTER=${JBX_AFTER:-3}

dim()  { printf '\033[2m%s\033[0m\n' "$*"; }
you()  { printf '\033[1m> %s\033[0m\n' "$*"; }
bot()  { printf '\033[38;5;208m●\033[0m %s\n' "$*"; }
beat() { sleep "${1:-0.6}"; }

# ONE TOOL CALL, THE WAY A HARNESS MAKES IT.
#
# The payload carries what Claude Code really sends: the command, the
# description it already asked the model for, and the timeout the caller
# was prepared to wait. jbx answers with the whole input object back —
# under Claude a field left out is a field deleted.
call() {
    local line=$1 why=$2
    bot "$why"
    dim "  $line"
    beat 0.4

    local payload answer wrapped
    payload=$(printf '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":%s,"description":%s,"timeout":600000}}' \
        "$(printf '%s' "$line" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')" \
        "$(printf '%s' "$why"  | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')")
    answer=$(printf '%s' "$payload" | "$JBX" hook claude)
    wrapped=$(printf '%s' "$answer" | python3 -c \
        'import json,sys; print(json.load(sys.stdin)["hookSpecificOutput"]["updatedInput"]["command"])')

    # AND THE LINE THE HOOK WROTE IS THE LINE THAT RUNS.
    eval "$wrapped"
    echo
}

clear
dim "a scripted reproduction — the agent's turn is acted, everything jbx"
dim "does is real. JBX_AFTER=$JBX_AFTER so the cut lands on camera."
echo

you "build the release binary and tell me what changed in the last commit"
beat 1
# NOT THE TEST SUITE. `JBX_AFTER=3` is exported, and the suite inherits
# it — several of its tests depend on the default cut, so demonstrating
# with `cargo test` filmed a failure caused by the demo itself. A build
# is long, honest, and indifferent to the threshold.
call "cargo build --release 2>&1 | tail -3" "Building the release binary"

bot "It let go, so I am not standing over it."
beat 0.8
call "git log -1 --format=%s%n%n%b | head -8" "Reading the last commit while that runs"

bot "Now I will wait for the one I left."
beat 0.6
dim "  jbx wait <id>   — ends when the job does, carries its exit code"
