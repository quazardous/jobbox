#!/bin/sh
# Put jbx in ~/.local/bin. No root, no package manager, nothing outside
# your home.
#
# `cargo install --path .` does the same thing and is a fine way to do
# it. This exists for the two things it cannot: `--symlink`, so a
# checkout you are working on IS the installed copy, and `--uninstall`,
# so removing it is one gesture and not a hunt.
set -eu

BIN="$HOME/.local/bin"
SRC="$(cd "$(dirname "$0")" && pwd)"
MODE=copy

for arg in "$@"; do
    case "$arg" in
        --symlink)   MODE=symlink ;;
        --uninstall) MODE=uninstall ;;
        -h|--help)
            echo "usage: ./install.sh [--symlink] [--uninstall]"
            echo "  --symlink    point the install at this checkout, so edits are live"
            echo "  --uninstall  remove the binary (your logs and readings stay)"
            exit 0 ;;
        *)
            echo "install.sh: unknown option $arg" >&2
            exit 2 ;;
    esac
done

if [ "$MODE" = uninstall ]; then
    # THE HOOKS COME OUT BEFORE THE BINARY DOES, and the order is the
    # whole point: a settings file pointing at a binary that is gone
    # breaks every shell command in every session that reads it, and the
    # error names a path rather than a cause. Measured, the hard way.
    if [ -x "$BIN/jbx" ]; then
        "$BIN/jbx" init --undo || echo "  (could not undo the hooks — check \`jbx init --undo\`)"
    fi
    rm -f "$BIN/jbx"
    echo "  removed $BIN/jbx"
    echo "  your logs and readings are untouched, in \${JBX_DIR:-~/.cache/jbx}."
    exit 0
fi

command -v cargo >/dev/null 2>&1 || {
    echo "install.sh: cargo is not on your PATH. jbx is a Rust program;" >&2
    echo "  see https://rustup.rs, or install a prebuilt binary yourself." >&2
    exit 1
}

echo "  building…"
( cd "$SRC" && cargo build --release --quiet )

mkdir -p "$BIN"
rm -f "$BIN/jbx"
if [ "$MODE" = symlink ]; then
    ln -s "$SRC/target/release/jbx" "$BIN/jbx"
    echo "  linked  $BIN/jbx -> $SRC/target/release/jbx"
    echo "  rebuild with \`cargo build --release\` and the install follows."
else
    cp "$SRC/target/release/jbx" "$BIN/jbx"
    echo "  copied  $BIN/jbx"
fi

case ":$PATH:" in
    *":$BIN:"*)
        echo "  $("$BIN/jbx" --version)"
        echo
        echo "  Next: \`jbx init\` declares its hooks — and takes rtk's over rather"
        echo "  than racing it. \`jbx init --undo\` puts everything back." ;;
    *)
        echo
        echo "  $BIN is NOT on your PATH. Until it is, the hooks jbx declares"
        echo "  will still work (they carry the full path) but you cannot type"
        echo "  \`jbx\`. Add it:"
        echo "      export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
esac
