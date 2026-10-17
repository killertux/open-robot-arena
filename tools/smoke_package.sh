#!/bin/sh
# The packaging acceptance test: unpack the tarball into a temporary directory
# and use it the way a player would.
#
#   ./tools/smoke_package.sh [dist/ra2-open-robot-arena-*.tar.gz]
#
# It asserts:
#   * the archive unpacks to one directory with the expected layout, no game files;
#   * bin/ra2-shell --route botlab --out DIR --game local/game is headless and
#     writes the seven frames;
#   * the boot frame is the main menu, byte for byte the same PNG that an
#     independent render of UI/mainmenu/mainmenu.gib produces;
#   * the missing-game error names RA2.cfz, UI and Scripts (direct and launcher);
#   * run.sh finds the game through RA2_GAME_DIR and its CPython 3.12 check
#     reports the library and stdlib it needs;
#   * the launcher's exhibition route writes its three frames.
#
# The game is read from local/game (or $RA2_GAME_DIR); the repo ships none, so
# this test cannot fake one. Set RA2_GAME_DIR to run it on another machine.

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
TARBALL=${1:-}
if [ -z "$TARBALL" ]; then
    TARBALL=$(ls -t "$ROOT"/dist/ra2-open-robot-arena-*.tar.gz 2>/dev/null | head -1 || true)
fi
[ -n "$TARBALL" ] && [ -f "$TARBALL" ] || {
    echo "smoke: no tarball (build one with tools/package.sh)" >&2
    exit 2
}

if [ -d "$ROOT/local/game" ]; then
    GAME=$ROOT/local/game
    GAME_ARG=local/game
    RUN_CWD=$ROOT
else
    GAME=${RA2_GAME_DIR:-}
    [ -n "$GAME" ] && [ -d "$GAME" ] || {
        echo "smoke: no game at local/game and RA2_GAME_DIR is not a directory" >&2
        echo "       the repository ships no game files; set RA2_GAME_DIR" >&2
        exit 2
    }
    GAME_ARG=$GAME
    RUN_CWD=$ROOT
fi

TMP=$(mktemp -d "${TMPDIR:-/tmp}/ra2-package-smoke.XXXXXX")
cleanup() {
    rc=$?
    if [ "$rc" -eq 0 ]; then
        rm -rf "$TMP"
    else
        echo "smoke: failure kept $TMP" >&2
    fi
}
trap cleanup EXIT

die() {
    echo "smoke: $*" >&2
    exit 1
}

echo "smoke: tarball $TARBALL"
tar -xzf "$TARBALL" -C "$TMP"
PKG=$(find "$TMP" -mindepth 1 -maxdepth 1 -type d | head -1)
[ -n "$PKG" ] || die "the archive has no top-level directory"
[ -x "$PKG/bin/ra2-shell" ] || die "bin/ra2-shell is missing"

for file in run.sh install.sh README.txt LICENSE-MIT LICENSE-APACHE NOTICE THIRD_PARTY.md \
    share/applications/ra2-open-robot-arena.desktop \
    share/icons/hicolor/256x256/apps/ra2-open-robot-arena.png \
    share/icons/hicolor/scalable/apps/ra2-open-robot-arena.svg \
    bin/ra2 bin/ra2-render bin/ra2-sim bin/ra2-duel; do
    [ -e "$PKG/$file" ] || die "the package is missing $file"
done

# A package must never contain the game.
if find "$PKG" \( -iname 'RA2.cfz' -o -iname '*.gmf' -o -iname '*.bot' -o -iname '*.gib' \) | grep -q .; then
    die "the package contains game data"
fi
[ ! -d "$PKG/UI" ] || die "the package contains a UI directory"

png_size() {
    python3 - "$1" <<'PY'
import struct, sys
with open(sys.argv[1], "rb") as f:
    head = f.read(24)
if head[:8] != b"\x89PNG\r\n\x1a\n" or head[12:16] != b"IHDR":
    raise SystemExit(f"{sys.argv[1]}: not a PNG")
w, h = struct.unpack(">II", head[16:24])
print(w, h)
PY
}

check_frames() {
    out=$1
    shift
    for frame in "$@"; do
        [ -s "$out/$frame" ] || die "missing frame $out/$frame"
        size=$(png_size "$out/$frame")
        [ "$size" = "1280 1005" ] || die "$frame is $size, expected 1280 1005"
    done
}

echo "== shell binary, botlab route, game at $GAME_ARG"
(
    cd "$RUN_CWD"
    "$PKG/bin/ra2-shell" --route botlab --out "$TMP/frames" --game "$GAME_ARG"
) >"$TMP/botlab.log" 2>&1 || {
    cat "$TMP/botlab.log" >&2
    die "bin/ra2-shell --route botlab failed"
}
cat "$TMP/botlab.log"
grep -q 'booted mainmenu/mainmenu.gib' "$TMP/botlab.log" || die "the boot screen is not the main menu"
grep -q 'finished on botlab/botlabwin.gib' "$TMP/botlab.log" || die "the botlab route did not finish in the workshop"

check_frames "$TMP/frames" \
    00-mainmenu.png 01-select-team.png 02-team-hq.png 03-robot-inventory.png \
    04-botlab-overview.png 05-botlab-chassis.png 06-chassis-step1.png
cmp -s "$TMP/frames/00-mainmenu.png" "$TMP/frames/01-select-team.png" &&
    die "the first click did not change the screen"

echo "== independent main-menu render"
(
    cd "$ROOT/engine"
    cargo run -q --offline --release -p ra2-render --example uishot -- \
        "$GAME/UI/mainmenu/mainmenu.gib" "$TMP/mainmenu-reference.png" 1280 1005
) >"$TMP/reference.log" 2>&1 || {
    cat "$TMP/reference.log" >&2
    die "could not render the reference main menu"
}
cat "$TMP/reference.log"
cmp "$TMP/mainmenu-reference.png" "$TMP/frames/00-mainmenu.png" ||
    die "the boot frame is not the independently rendered main menu"
echo "smoke: boot frame == mainmenu.gib (sha256 $(sha256sum "$TMP/frames/00-mainmenu.png" | cut -d' ' -f1))"

echo "== missing game, direct binary"
set +e
(
    cd "$RUN_CWD"
    "$PKG/bin/ra2-shell" --route botlab --out "$TMP/missing-out" --game "$TMP/not-a-game"
) >"$TMP/missing.log" 2>&1
rc=$?
set -e
[ "$rc" -ne 0 ] || die "the binary accepted a missing game"
grep -q 'RA2.cfz' "$TMP/missing.log" || die "the missing-game error does not name RA2.cfz"
grep -q 'UI' "$TMP/missing.log" || die "the missing-game error does not name UI"
grep -q 'Scripts' "$TMP/missing.log" || die "the missing-game error does not name Scripts"
cat "$TMP/missing.log"

echo "== launcher, RA2_GAME_DIR, exhibition route"
RA2_GAME_DIR="$GAME" "$PKG/run.sh" --route exhibition --out "$TMP/launcher-frames" \
    >"$TMP/launcher.log" 2>&1 || {
    cat "$TMP/launcher.log" >&2
    die "run.sh --route exhibition failed"
}
cat "$TMP/launcher.log"
grep -q 'booted mainmenu/mainmenu.gib' "$TMP/launcher.log" || die "run.sh did not boot the main menu"
check_frames "$TMP/launcher-frames" \
    00-mainmenu.png 01-exhibition-setup.png 02-arena-picker.png

echo "== launcher, CPython 3.12 check"
"$PKG/run.sh" --check-python >"$TMP/python.log" 2>&1 || {
    cat "$TMP/python.log" >&2
    die "run.sh --check-python failed"
}
cat "$TMP/python.log"
grep -q 'libpython3.12.so.1.0: found' "$TMP/python.log" ||
    die "the launcher did not find libpython3.12.so.1.0"
grep -q 'CPython 3.12 stdlib: found' "$TMP/python.log" ||
    die "the launcher did not find the CPython 3.12 stdlib"
grep -Eq 'python3\.12 [0-9]+\.[0-9]+\.[0-9]+' "$TMP/python.log" ||
    die "the launcher did not report a python3.12 version"

echo "== launcher, missing game"
set +e
env -u RA2_GAME_DIR "$PKG/run.sh" --game "$TMP/not-a-game" --route botlab \
    --out "$TMP/launcher-missing" >"$TMP/launcher-missing.log" 2>&1
rc=$?
set -e
[ "$rc" -ne 0 ] || die "the launcher accepted a missing game"
grep -q 'RA2.cfz' "$TMP/launcher-missing.log" || die "the launcher error does not name RA2.cfz"
grep -q 'UI' "$TMP/launcher-missing.log" || die "the launcher error does not name UI"
grep -q 'Scripts' "$TMP/launcher-missing.log" || die "the launcher error does not name Scripts"
cat "$TMP/launcher-missing.log"

echo
echo "smoke: PASS — 7 botlab frames, 3 exhibition frames, boot frame is the main menu"
