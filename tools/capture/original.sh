#!/bin/sh
# Capture the original Robot Arena 2 on a Xephyr display.
#
#   tools/capture/original.sh                 # 1024x768 on :3
#   RA2_ORIGINAL_SIZE=1280x1024 RA2_CAPTURE_DISPLAY=:2 tools/capture/original.sh
#   RA2_CAPTURE_XEPHYR=1 tools/capture/original.sh   # start the display first
#
# What this script can and cannot do on this machine (measured 2026-10-18,
# documented in docs/25-windowed.md §4):
#
#   * It can reach the **main menu**: the two startup dialogs are dismissed with
#     Return, which Wine receives through the normal keyboard path.
#   * It can capture the menu at either shipped size.
#   * It can **not** drive the menu with the mouse.  RA2 opens DirectInput8
#     with `SetCooperativeLevel(..., 0x5)` (exclusive | foreground) and polls it
#     with `GetDeviceData`; Wine's dinput mouse ignores XTEST, XWarpPointer,
#     XSendEvent, a host-pointer event forwarded by Xephyr, `SetCursorPos` /
#     `mouse_event` from a Wine process, and a /dev/uinput virtual mouse.  The
#     dinput trace shows `Returning 0 events queued` throughout.  A window
#     manager makes the fullscreen game draw a blank frame, and windowed mode
#     also renders blank under llvmpipe.
#
# So this script always captures `mainmenu__<size>__<seq>.png`, tries one click
# (documented in `after-click.png` or `click-failed.txt`), and never labels a
# screen it did not reach.  `tools/capture/wininput.py` and
# `tools/capture/uinput_mouse.py` are the two injection experiments; run them
# by hand to reproduce the trace.
#
# Environment:
#   RA2_CAPTURE_DISPLAY   Xephyr display (default :3)
#   RA2_CAPTURE_XEPHYR    start Xephyr on that display if not live
#   RA2_ORIGINAL_SIZE     1024x768 (default) or 1280x1024
#   RA2_ORIGINAL_CLICK_X  click x (default: the EXHIBITION centre for the size)
#   RA2_ORIGINAL_CLICK_Y  click y
#   RA2_ORIGINAL_SEQ      fixture sequence (default: next free three digits)
#   RA2_ORACLE_LOCAL      local/ directory (default: main worktree)
#   RA2_ORIGINAL_TIMEOUT  seconds to wait for the menu (default 40)
#
# The installed game's ra2.cfg is backed up and restored with a trap.  Nothing
# from the game directory is copied into the repository; captures go to the
# ignored <local>/oracle/original/.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd -P)
XTEST=$SCRIPT_DIR/xtest.py
XWD2PNG=$SCRIPT_DIR/xwd2png.py
PNGINFO=$SCRIPT_DIR/pnginfo.py

die() { echo "original: $*" >&2; exit 2; }

MAIN_ROOT=$ROOT
if COMMON=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
    MAIN_ROOT=$(dirname -- "$COMMON")
fi
LOCAL=${RA2_ORACLE_LOCAL:-$MAIN_ROOT/local}
ORIG=$LOCAL/oracle/original
DISPLAY_NAME=${RA2_CAPTURE_DISPLAY:-:3}
SIZE=${RA2_ORIGINAL_SIZE:-1024x768}
WIDTH=${SIZE%x*}
HEIGHT=${SIZE#*x}
case $WIDTH$HEIGHT in *[!0-9]*) die "RA2_ORIGINAL_SIZE must be WxH" ;; esac

case $SIZE in
    1024x768) DEFAULT_X=514; DEFAULT_Y=346 ;;
    1280x1024) DEFAULT_X=640; DEFAULT_Y=478 ;;
    *) DEFAULT_X=$((WIDTH * 502 / 1000)); DEFAULT_Y=$((HEIGHT * 46 / 100)) ;;
esac
CLICK_X=${RA2_ORIGINAL_CLICK_X:-$DEFAULT_X}
CLICK_Y=${RA2_ORIGINAL_CLICK_Y:-$DEFAULT_Y}
TIMEOUT=${RA2_ORIGINAL_TIMEOUT:-40}

[ -d "$LOCAL/game/UI" ] || die "no game data at $LOCAL/game (set RA2_ORACLE_LOCAL)"
mkdir -p -- "$ORIG"
mkdir -p -- "$LOCAL/oracle/log"

PREFIX="$HOME/.var/app/org.winehq.Wine/data/wine"
GAMEDIR="$PREFIX/drive_c/Program Files (x86)/Robot Arena 2"
[ -f "$GAMEDIR/Robot Arena 2.exe" ] || die "the game is not installed in $GAMEDIR"
CFG_BACKUP=$LOCAL/oracle/log/ra2.cfg.before
cp -f "$GAMEDIR/ra2.cfg" "$CFG_BACKUP"

SEQ=${RA2_ORIGINAL_SEQ:-}
if [ -z "$SEQ" ]; then
    for n in 000 001 002 003 004 005 006 007 008 009; do
        [ -e "$ORIG/mainmenu__${SIZE}__${n}.png" ] || { SEQ=$n; break; }
    done
fi
[ -n "$SEQ" ] || die "no free fixture sequence for mainmenu__${SIZE}"

XEPHYR_PID=
APP_PID=
cleanup() {
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null || true
    [ -n "$XEPHYR_PID" ] && kill "$XEPHYR_PID" 2>/dev/null || true
    flatpak kill org.winehq.Wine >/dev/null 2>&1 || true
    cp -f "$CFG_BACKUP" "$GAMEDIR/ra2.cfg" || true
}
trap cleanup EXIT INT TERM

if [ -n "${RA2_CAPTURE_XEPHYR:-}" ] && ! DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1; then
    Xephyr "$DISPLAY_NAME" -screen "$SIZE" -ac -noreset \
        >"$LOCAL/oracle/log/xephyr-${WIDTH}x${HEIGHT}.log" 2>&1 &
    XEPHYR_PID=$!
    for _ in $(seq 1 40); do
        DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1 && break
        sleep 0.25
    done
fi
DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1 || die "display $DISPLAY_NAME is not live"
DIMS=$(DISPLAY=$DISPLAY_NAME xdpyinfo | awk '/dimensions:/ {print $2; exit}')
[ "$DIMS" = "$SIZE" ] || die "display $DISPLAY_NAME is $DIMS, but RA2_ORIGINAL_SIZE is $SIZE"

# Set the shipped fullscreen size, as tools/oracle/README.md documents.
sed -i \
    -e "s/^FullScreen=.*/FullScreen=1/" \
    -e "s/^FSDisplayWidth=.*/FSDisplayWidth=$WIDTH/" \
    -e "s/^FSDisplayHeight=.*/FSDisplayHeight=$HEIGHT/" \
    -e "s/^FS3DDisplayWidth=.*/FS3DDisplayWidth=$WIDTH/" \
    -e "s/^FS3DDisplayHeight=.*/FS3DDisplayHeight=$HEIGHT/" \
    "$GAMEDIR/ra2.cfg"

DISPLAY=$DISPLAY_NAME flatpak run --command=sh org.winehq.Wine -c \
    'cd "$WINEPREFIX/drive_c/Program Files (x86)/Robot Arena 2" && exec wine "Robot Arena 2.exe"' \
    >"$LOCAL/oracle/log/original-${WIDTH}x${HEIGHT}.log" 2>&1 &
APP_PID=$!

# Wait for the game window, then dismiss the two startup dialogs with Return
# (the keyboard path works; the mouse path is the one that does not).
WID=
for _ in $(seq 1 "$TIMEOUT"); do
    if FOUND=$(DISPLAY=$DISPLAY_NAME python3 "$XTEST" find --title "Robot Arena 2" 2>/dev/null); then
        WID=$(printf '%s\n' "$FOUND" | head -n 1 | awk '{print $1}')
        break
    fi
    sleep 1
done
[ -n "$WID" ] || die "the game did not open a window"

DISPLAY=$DISPLAY_NAME python3 "$XTEST" focus --title "Robot Arena 2" >/dev/null 2>&1 || true
sleep 2
DISPLAY=$DISPLAY_NAME python3 "$XTEST" key Return >/dev/null
sleep 5
DISPLAY=$DISPLAY_NAME python3 "$XTEST" key Return >/dev/null
sleep 12

capture() {
    out=$1
    DISPLAY=$DISPLAY_NAME xwd -root -silent -out "$out.xwd"
    python3 "$XWD2PNG" "$out.xwd" "$out.png" --quiet >/dev/null
    rm -f "$out.xwd"
}

MENU=$ORIG/mainmenu__${SIZE}__${SEQ}
capture "$MENU"
python3 "$PNGINFO" "$MENU.png" --label "mainmenu $SIZE" --nonuniform --expect "$SIZE"

# One click attempt, recorded.  The screen is allowed to stay the menu: that is
# the measured result on this Wine build, and the log says exactly what was
# tried.
DISPLAY=$DISPLAY_NAME python3 "$XTEST" focus --title "Robot Arena 2" >/dev/null 2>&1 || true
DISPLAY=$DISPLAY_NAME python3 "$XTEST" click "$CLICK_X" "$CLICK_Y" --hold 0.15 --settle 0.2 \
    >"$LOCAL/oracle/log/click-${WIDTH}x${HEIGHT}.txt" 2>&1 || true
sleep 1.5
AFTER=$LOCAL/oracle/log/after-click__${SIZE}.png
capture "${AFTER%.png}"
MAE=$(python3 "$ROOT/tools/oracle/compare.py" "$MENU.png" "$AFTER" \
    "$LOCAL/oracle/log/after-click__${SIZE}__diff.png" 2>/dev/null | python3 -c \
    'import json,sys; print(json.load(sys.stdin)["mae"]["all"])')
if python3 -c "import sys; sys.exit(0 if $MAE > 0.5 else 1)"; then
    echo "original: click at $CLICK_X,$CLICK_Y changed the screen (MAE $MAE); saved log/after-click__${SIZE}.png"
    echo "          this is an unidentified post-menu screen, not a fixture claim"
else
    cat >"$ORIG/mainmenu__${SIZE}__${SEQ}.click-failed.txt" <<EOF
click at $CLICK_X,$CLICK_Y did not change the menu (MAE $MAE).
Method: XTEST XTestFakeMotionEvent + XTestFakeButtonEvent after XSetInputFocus.
Reason measured with WINEDEBUG=+dinput: DirectInput8 exclusive mouse,
GetDeviceData returns 0 events for XTEST, XWarpPointer, XSendEvent, host
pointer through Xephyr, Wine SetCursorPos/mouse_event, and /dev/uinput.
See docs/25-windowed.md §4.
EOF
    echo "original: mainmenu captured; click at $CLICK_X,$CLICK_Y did not activate (MAE $MAE)"
    echo "          see $ORIG/mainmenu__${SIZE}__${SEQ}.click-failed.txt"
fi

echo "original: saved $MENU.png"
