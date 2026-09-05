#!/bin/sh
# Put jbx in ~/.local/bin.
#
#   curl -fsSL https://raw.githubusercontent.com/quazardous/jobbox/main/install.sh | sh
#
# By default it downloads the binary for this machine from the latest
# release and checks it against the published sums. Nothing is compiled,
# nothing needs root, and nothing is installed outside your home.
#
#   --from-source   build this checkout instead (needs cargo)
#   --version vX.Y  a particular release rather than the latest
#   --symlink       with --from-source: point the install at the checkout
#   --uninstall     remove it; your logs and readings stay
set -eu

REPO=quazardous/jobbox
BIN="${JBX_BIN:-$HOME/.local/bin}"
SRC=$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo .)
MODE=release
VERSION=

for arg in "$@"; do
    case "$arg" in
        --from-source) MODE=source ;;
        --symlink)     MODE=symlink ;;
        --uninstall)   MODE=uninstall ;;
        --version=*)   VERSION=${arg#--version=} ;;
        -h|--help)
            # PRINTED, NOT READ BACK FROM THE FILE. Piped from curl there
            # is no file to read: `$0` is `sh`.
            cat <<'USAGE'
install.sh — put jbx in ~/.local/bin

  curl -fsSL https://raw.githubusercontent.com/quazardous/jobbox/main/install.sh | sh

By default it downloads the binary for this machine from the latest
release and checks it against the published sums.

  --from-source     build this checkout instead (needs cargo)
  --symlink         build, and point the install at the checkout
  --version=vX.Y.Z  a particular release rather than the latest
  --uninstall       remove it; your logs and readings stay

  JBX_BIN           where to put it (default ~/.local/bin)
USAGE
            exit 0 ;;
        *)
            echo "install.sh: unknown option $arg" >&2
            exit 2 ;;
    esac
done

say() { echo "  $*"; }
die() { echo "install.sh: $*" >&2; exit 1; }

if [ "$MODE" = uninstall ]; then
    # THE HOOKS COME OUT BEFORE THE BINARY DOES, and the order is the
    # whole point: a settings file pointing at a binary that is gone
    # breaks every shell command in every session that reads it, and the
    # error names a path rather than a cause.
    if [ -x "$BIN/jbx" ]; then
        "$BIN/jbx" init --undo || say "(could not undo the hooks — check \`jbx init --undo\`)"
    fi
    rm -f "$BIN/jbx"
    say "removed $BIN/jbx"
    say "your logs and readings are untouched, in \${JBX_DIR:-~/.cache/jbx}."
    exit 0
fi

# ── BUILDING, FOR WHOEVER HAS THE CHECKOUT ──────────────────────────────
if [ "$MODE" = source ] || [ "$MODE" = symlink ]; then
    [ -f "$SRC/Cargo.toml" ] || die "no checkout here — building needs the repository.
  Run it without --from-source to download the binary instead."
    command -v cargo >/dev/null 2>&1 || die "cargo is not on your PATH.
  Run it without --from-source to download the binary instead."
    say "building…"
    ( cd "$SRC" && cargo build --release --quiet )
    mkdir -p "$BIN"
    rm -f "$BIN/jbx"
    if [ "$MODE" = symlink ]; then
        ln -s "$SRC/target/release/jbx" "$BIN/jbx"
        say "linked  $BIN/jbx -> $SRC/target/release/jbx"
    else
        cp "$SRC/target/release/jbx" "$BIN/jbx"
        say "copied  $BIN/jbx"
    fi
else
    # ── DOWNLOADING, WHICH IS THE DEFAULT ───────────────────────────────
    #
    # WHICH BINARY. musl on Linux because it is statically linked, so the
    # distribution that built it does not have to match yours.
    os=$(uname -s)
    arch=$(uname -m)
    case "$os-$arch" in
        Linux-x86_64)          asset=linux-x86_64-musl ;;
        Darwin-arm64)          asset=macos-arm64 ;;
        Darwin-x86_64)         asset=macos-x86_64 ;;
        *)
            die "no published binary for $os-$arch.
  Build it instead: git clone https://github.com/$REPO && cd jobbox && ./install.sh --from-source" ;;
    esac

    command -v curl >/dev/null 2>&1 || die "curl is needed to download a release."

    if [ -z "$VERSION" ]; then
        # The latest tag, read from the API. `grep` rather than `jq`,
        # because an installer that needs a dependency installed first is
        # not much of an installer.
        # READ IT WHOLE, THEN PARSE. `curl … | grep -m1` makes grep leave
        # at the first match, and curl then prints "(23) Failure writing
        # output" over an install that worked perfectly.
        latest=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")
        VERSION=$(printf '%s\n' "$latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
        [ -n "$VERSION" ] || die "could not find the latest release. Pass --version=vX.Y.Z."
    fi

    name="jbx-$VERSION-$asset.tar.gz"
    tmp=$(mktemp -d)
    # WHATEVER HAPPENS, THE SCRATCH GOES. A half-downloaded archive left
    # in /tmp is the kind of litter nobody ever finds again.
    trap 'rm -rf "$tmp"' EXIT INT TERM

    say "fetching $VERSION for $asset…"
    curl -fsSL -o "$tmp/$name" \
        "https://github.com/$REPO/releases/download/$VERSION/$name" \
        || die "no such release asset: $name"

    # CHECKED AGAINST THE PUBLISHED SUMS. TLS says the bytes came from
    # GitHub; it does not say they are the bytes that release built. If
    # the sums are missing — an older release — say so and carry on
    # rather than refusing, but never say nothing.
    if curl -fsSL -o "$tmp/SHA256SUMS" \
        "https://github.com/$REPO/releases/download/$VERSION/SHA256SUMS" 2>/dev/null; then
        if command -v sha256sum >/dev/null 2>&1; then
            ( cd "$tmp" && grep " $name\$" SHA256SUMS | sha256sum -c - >/dev/null ) \
                || die "the download does not match the published sum. Nothing was installed."
            say "checksum ok"
        elif command -v shasum >/dev/null 2>&1; then
            ( cd "$tmp" && grep " $name\$" SHA256SUMS | shasum -a 256 -c - >/dev/null ) \
                || die "the download does not match the published sum. Nothing was installed."
            say "checksum ok"
        else
            say "no sha256 tool here — the download was NOT verified."
        fi
    else
        say "$VERSION publishes no sums — the download was NOT verified."
    fi

    tar xzf "$tmp/$name" -C "$tmp"
    mkdir -p "$BIN"
    rm -f "$BIN/jbx"
    cp "$tmp/jbx-$VERSION-$asset/jbx" "$BIN/jbx"
    chmod +x "$BIN/jbx"
    say "installed $BIN/jbx"
fi

say "$("$BIN/jbx" --version)"

case ":$PATH:" in
    *":$BIN:"*)
        echo
        say "Next: \`jbx init\` declares its hooks — and takes rtk's over rather"
        say "than racing it. \`jbx why\` says what it does and why." ;;
    *)
        echo
        say "$BIN is NOT on your PATH. The hooks jbx declares will still work"
        say "(they carry the full path) but you cannot type \`jbx\`. Add it:"
        say "    export PATH=\"$BIN:\$PATH\"" ;;
esac
