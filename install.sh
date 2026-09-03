#!/usr/bin/env bash
# Install jobbox into ~/.local — no root, no system package.
#
#   ./install.sh              copy this checkout into ~/.local/lib/jobbox
#   ./install.sh --symlink    point at this checkout instead (dev install:
#                             edits here are live, no reinstall)
#   ./install.sh --uninstall  remove what we installed (logs are kept)
#   ./install.sh --uninstall --purge   ALSO delete ~/.cache/jobbox
#
# Reentrant: running it again over an existing install is safe and
# replaces the code in place. It never touches the queue, the logs or the
# pending signals — those live in ~/.cache/jobbox and outlive the tool.
#
# WHY USER SPACE. jobbox queues a user's own commands and writes into
# that user's cache; nothing it does needs root, and asking for root to
# install a wrapper around a queue would be the wrong trade. `~/.local`
# is also what makes `jobbox` a command rather than a path to a script —
# which is what the README has always shown.
set -euo pipefail

LIB="$HOME/.local/lib/jobbox"
BIN="$HOME/.local/bin"
CACHE="$HOME/.cache/jobbox"
SRC="$(cd "$(dirname "$0")" && pwd)"

SYMLINK=false
UNINSTALL=false
PURGE=false

while [ $# -gt 0 ]; do
    case "$1" in
        --symlink)   SYMLINK=true; shift ;;
        --uninstall) UNINSTALL=true; shift ;;
        --purge)     PURGE=true; shift ;;
        -h|--help)   sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *)           echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if $UNINSTALL; then
    rm -f  "$BIN/jobbox"
    rm -rf "$LIB"
    echo "  removed $BIN/jobbox and $LIB"
    if $PURGE; then
        rm -rf "$CACHE"
        echo "  removed $CACHE — logs and pending signals are gone"
    else
        echo "  kept    $CACHE (logs, pending signals) — --purge to delete"
    fi
    echo
    echo "  The task-spooler daemon is still running; it is not ours to stop."
    echo "  \`tsp -K\` ends it, and anything queued with it."
    exit 0
fi

# ── the dependencies, named before we do anything ───────────────────────
#
# REFUSED EARLY AND BY NAME. An install that succeeds and then fails at
# first use teaches nothing about which of the two pieces is missing.
missing=""
command -v python3 >/dev/null 2>&1 || missing="$missing python3"
command -v tsp     >/dev/null 2>&1 || missing="$missing task-spooler"
if [ -n "$missing" ]; then
    echo "  missing:$missing" >&2
    echo >&2
    echo "  Fedora: sudo dnf install task-spooler" >&2
    echo "  Debian: sudo apt install task-spooler" >&2
    echo "  macOS:  brew install task-spooler" >&2
    exit 1
fi

mkdir -p "$BIN"
rm -rf "$LIB"
mkdir -p "$(dirname "$LIB")"

if $SYMLINK; then
    ln -s "$SRC" "$LIB"
    echo "  linked  $LIB -> $SRC"
else
    mkdir -p "$LIB"
    cp "$SRC/jobbox.py" "$SRC/jobbox-onfinish" "$LIB/"
    chmod +x "$LIB/jobbox.py" "$LIB/jobbox-onfinish"
    # THE SKILL TRAVELS WITH THE TOOL. `jobbox init` reads it from here;
    # leaving it behind would install the verbs without the judgement.
    cp -r "$SRC/skills" "$LIB/"
    echo "  copied  jobbox.py, jobbox-onfinish, skills -> $LIB"
fi

# THE LAUNCHER IS A FILE, NOT A SYMLINK TO jobbox.py. `TS_ONFINISH` is
# resolved from `jobbox.py`'s own location, so the script must be reached
# by its real path — a symlink on the PATH would work, but this keeps the
# indirection in one readable place.
cat > "$BIN/jobbox" <<LAUNCHER
#!/bin/sh
# Installed by jobbox's install.sh — edit $LIB instead of this file.
exec python3 "$LIB/jobbox.py" "\$@"
LAUNCHER
chmod +x "$BIN/jobbox"
echo "  wrote   $BIN/jobbox"

echo
# THE CHECK HAS TO BE ABLE TO FAIL. This line used to end in `|| true`,
# calling an option that did not exist — so it reported success whatever
# happened, which is worse than not checking at all.
case ":$PATH:" in
    *":$BIN:"*) if version=$("$BIN/jobbox" --version 2>&1); then
                    echo "  $version is on your PATH. Try: jobbox health"
                else
                    echo "  installed, but it does not run:" >&2
                    echo "      $version" >&2
                    exit 1
                fi ;;
    *)          echo "  $BIN is NOT on your PATH. Add it:"
                echo "      export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
esac
echo "  Then, inside a project:  jobbox init"
