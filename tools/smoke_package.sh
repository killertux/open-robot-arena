#!/bin/sh
# The packaging acceptance test: unpack the tarball into a temporary directory
# and use it the way a player would.
#
#   ./tools/smoke_package.sh [dist/ra2-open-robot-arena-*.tar.gz]
#
# It asserts:
#   * the archive unpacks to one directory with the expected layout, no game files;
#   * the packaged client, bin/ra2-app --offscreen PNG --game local/game, boots
#     the main menu and writes that frame;
#   * that frame is byte for byte the same PNG an independent render of
#     UI/mainmenu/mainmenu.gib produces (the uishot example, not the app);
#   * the CLI path still works: bin/ra2-shell --route botlab writes its seven
#     frames and logs the boot and finish screens;
#   * the missing-game error names RA2.cfz, UI and Scripts (direct and launcher);
#   * run.sh finds the game through RA2_GAME_DIR, runs the client headlessly and
#     its CPython 3.12 check reports the library and stdlib it needs;
#   * the launcher refuses a windowed run with no display and points at
#     --offscreen.
#
# The game is read from local/game (or $RA2_GAME_DIR); the repo ships none, so
# this test cannot fake one. Set RA2_GAME_DIR to run it on another machine.

set -eu

# The independent `uishot` render below is a Cargo invocation; keep its build
# scripts offline too (see tools/package.sh).
CARGO_NET_OFFLINE=true
export CARGO_NET_OFFLINE

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
TARBALL=${1:-}
if [ -z "$TARBALL" ]; then
    TARBALL=$(ls -t "$ROOT"/dist/ra2-open-robot-arena-*.tar.gz 2>/dev/null | head -1 || true)
fi
[ -n "$TARBALL" ] && [ -f "$TARBALL" ] || {
    echo "smoke: no tarball (build one with tools/package.sh)" >&2
    exit 2
}

if [ -n "${RA2_GAME_DIR:-}" ] && [ -d "$RA2_GAME_DIR" ]; then
    GAME=$RA2_GAME_DIR
    GAME_ARG=$GAME
    RUN_CWD=$ROOT
elif [ -d "$ROOT/local/game" ]; then
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

# The smoke test never writes to the player's real save folder or the install.
ORA_SAVE_DIR=$TMP/save
export ORA_SAVE_DIR

die() {
    echo "smoke: $*" >&2
    exit 1
}

echo "smoke: tarball $TARBALL"
tar -xzf "$TARBALL" -C "$TMP"
PKG=$(find "$TMP" -mindepth 1 -maxdepth 1 -type d | head -1)
[ -n "$PKG" ] || die "the archive has no top-level directory"
[ -x "$PKG/bin/ra2-app" ] || die "bin/ra2-app is missing"

for file in run.sh install.sh README.txt LICENSE-MIT LICENSE-APACHE NOTICE THIRD_PARTY.md \
    share/applications/ra2-open-robot-arena.desktop \
    share/icons/hicolor/256x256/apps/ra2-open-robot-arena.png \
    share/icons/hicolor/scalable/apps/ra2-open-robot-arena.svg \
    bin/ra2-shell bin/ra2 bin/ra2-render bin/ra2-sim bin/ra2-duel; do
    [ -e "$PKG/$file" ] || die "the package is missing $file"
done

# The launcher and the desktop entry must start the client, not the old shell.
grep -q 'bin/ra2-app' "$PKG/run.sh" || die "run.sh does not run bin/ra2-app"
grep -q '^Exec=ra2-open-robot-arena$' "$PKG/share/applications/ra2-open-robot-arena.desktop" ||
    die "the desktop entry does not run the launcher"

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

check_png() {
    [ -s "$1" ] || die "missing frame $1"
    size=$(png_size "$1")
    [ "$size" = "1280 1005" ] || die "$1 is $size, expected 1280 1005"
}

check_frames() {
    out=$1
    shift
    for frame in "$@"; do
        check_png "$out/$frame"
    done
}

echo "== ra2-app, offscreen main menu, game at $GAME_ARG"
(
    cd "$RUN_CWD"
    env -u DISPLAY -u WAYLAND_DISPLAY \
        "$PKG/bin/ra2-app" --game "$GAME_ARG" --offscreen "$TMP/app-mainmenu.png"
) >"$TMP/app.log" 2>&1 || {
    cat "$TMP/app.log" >&2
    die "bin/ra2-app --offscreen failed"
}
cat "$TMP/app.log"
grep -q 'booted mainmenu/mainmenu.gib' "$TMP/app.log" || die "the client did not boot the main menu"
grep -q 'finished on mainmenu/mainmenu.gib' "$TMP/app.log" || die "the client did not finish on the main menu"
check_png "$TMP/app-mainmenu.png"

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
cmp "$TMP/mainmenu-reference.png" "$TMP/app-mainmenu.png" ||
    die "the app's boot frame is not the independently rendered main menu"
echo "smoke: app boot frame == mainmenu.gib (sha256 $(sha256sum "$TMP/app-mainmenu.png" | cut -d' ' -f1))"

echo "== ra2-shell CLI, botlab route, game at $GAME_ARG"
(
    cd "$RUN_CWD"
    "$PKG/bin/ra2-shell" --route botlab --out "$TMP/frames" --game "$GAME_ARG"
) >"$TMP/botlab.log" 2>&1 || {
    cat "$TMP/botlab.log" >&2
    die "bin/ra2-shell --route botlab failed"
}
cat "$TMP/botlab.log"
grep -q 'booted mainmenu/mainmenu.gib' "$TMP/botlab.log" || die "the shell did not boot the main menu"
grep -q 'finished on botlab/botlabwin.gib' "$TMP/botlab.log" || die "the botlab route did not finish in the workshop"

check_frames "$TMP/frames" \
    00-mainmenu.png 01-select-team.png 02-team-hq.png 03-robot-inventory.png \
    04-botlab-overview.png 05-botlab-chassis.png 06-chassis-step1.png
cmp -s "$TMP/frames/00-mainmenu.png" "$TMP/frames/01-select-team.png" &&
    die "the first click did not change the screen"

echo "== missing game, direct client"
set +e
(
    cd "$RUN_CWD"
    env -u DISPLAY -u WAYLAND_DISPLAY \
        "$PKG/bin/ra2-app" --game "$TMP/not-a-game" --offscreen "$TMP/missing-out.png"
) >"$TMP/missing.log" 2>&1
rc=$?
set -e
[ "$rc" -ne 0 ] || die "the client accepted a missing game"
grep -q 'RA2.cfz' "$TMP/missing.log" || die "the missing-game error does not name RA2.cfz"
grep -q 'UI' "$TMP/missing.log" || die "the missing-game error does not name UI"
grep -q 'Scripts' "$TMP/missing.log" || die "the missing-game error does not name Scripts"
cat "$TMP/missing.log"

echo "== launcher, no display, windowed run"
set +e
env -u DISPLAY -u WAYLAND_DISPLAY RA2_GAME_DIR="$GAME" \
    "$PKG/run.sh" >"$TMP/no-display.log" 2>&1
rc=$?
set -e
[ "$rc" -ne 0 ] || die "the launcher tried to open a window without a display"
grep -q 'DISPLAY and WAYLAND_DISPLAY are both unset' "$TMP/no-display.log" ||
    die "the launcher's no-display message is missing"
grep -q -- '--offscreen' "$TMP/no-display.log" ||
    die "the launcher's no-display message does not mention --offscreen"
cat "$TMP/no-display.log"

echo "== launcher, RA2_GAME_DIR, offscreen client"
env -u DISPLAY -u WAYLAND_DISPLAY RA2_GAME_DIR="$GAME" \
    "$PKG/run.sh" --offscreen "$TMP/launcher-mainmenu.png" \
    >"$TMP/launcher.log" 2>&1 || {
    cat "$TMP/launcher.log" >&2
    die "run.sh --offscreen failed"
}
cat "$TMP/launcher.log"
grep -q 'booted mainmenu/mainmenu.gib' "$TMP/launcher.log" || die "run.sh did not boot the main menu"
check_png "$TMP/launcher-mainmenu.png"
cmp "$TMP/mainmenu-reference.png" "$TMP/launcher-mainmenu.png" ||
    die "the launcher's frame is not the independently rendered main menu"

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
env -u RA2_GAME_DIR "$PKG/run.sh" --game "$TMP/not-a-game" \
    --offscreen "$TMP/launcher-missing.png" >"$TMP/launcher-missing.log" 2>&1
rc=$?
set -e
[ "$rc" -ne 0 ] || die "the launcher accepted a missing game"
grep -q 'RA2.cfz' "$TMP/launcher-missing.log" || die "the launcher error does not name RA2.cfz"
grep -q 'UI' "$TMP/launcher-missing.log" || die "the launcher error does not name UI"
grep -q 'Scripts' "$TMP/launcher-missing.log" || die "the launcher error does not name Scripts"
cat "$TMP/launcher-missing.log"

echo
echo "smoke: PASS — ra2-app boot frame is the main menu; 7 botlab frames; launcher ok"
