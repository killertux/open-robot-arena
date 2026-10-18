#!/bin/sh
# Verify the windowed client with synthetic X input, and capture its screens.
#
#   tools/capture/window_smoke.sh
#
# The route is the shipped one (docs/21-play.md): main menu -> EXHIBITION ->
# ARENA -> an arena row -> START GAME, then Escape for the pause dialog.  Every
# click is a real XTEST pointer event at the window's root coordinates, not a
# `--click-names` shortcut, and each screen is captured with `xwd -id` and
# decoded by `xwd2png.py`.  The app's own stdout is the proof that a click hit:
# it prints `click X Y -> screen [Widget "name"]`.
#
# Environment:
#   RA2_CAPTURE_DISPLAY   X display (default :0).  On this machine use :0, or a
#                         1280x1024 Xephyr started by RA2_CAPTURE_XEPHYR=1.
#   RA2_CAPTURE_XEPHYR    start Xephyr on that display if it is not live
#   RA2_CAPTURE_EXPECT    expected window size, e.g. 1280x1024 (default: the
#                         size the window reports)
#   RA2_CAPTURE_OUT       output directory (default <local>/oracle/window)
#   RA2_GAME              game install (default <repo>/local/game)
#   RA2_CAPTURE_APP       the client binary (default engine/target/release/ra2-app)
#
# The offscreen render is compared with the captured menu after removing the
# fullscreen letterbox bars: the client draws its 1280x1005 scene centred in the
# window, so a 1280x1024 window has 9.5 black rows above and below.
#
# Exits 0 only when every capture has the expected size, is not uniform, every
# step changed the app's screen, and the pause frame differs from the match.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd -P)
ENGINE=$ROOT/engine
XTEST=$SCRIPT_DIR/xtest.py
XWD2PNG=$SCRIPT_DIR/xwd2png.py
PNGINFO=$SCRIPT_DIR/pnginfo.py
PNGCROP=$SCRIPT_DIR/pngcrop.py

die() {
    echo "window_smoke: $*" >&2
    exit 2
}

MAIN_ROOT=$ROOT
if COMMON=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
    MAIN_ROOT=$(dirname -- "$COMMON")
fi
LOCAL=${RA2_ORACLE_LOCAL:-$MAIN_ROOT/local}
OUT=${RA2_CAPTURE_OUT:-$LOCAL/oracle/window}
GAME=${RA2_GAME:-$LOCAL/game}
APP=${RA2_CAPTURE_APP:-$ENGINE/target/release/ra2-app}
DISPLAY_NAME=${RA2_CAPTURE_DISPLAY:-:0}
EXPECT=${RA2_CAPTURE_EXPECT:-}

[ -d "$GAME/UI" ] || die "no game data at $GAME (set RA2_GAME)"
[ -x "$APP" ] || die "no client at $APP (cargo build --release -p ra2-app)"
mkdir -p -- "$OUT"

XEPHYR_PID=
APP_PID=
cleanup() {
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null || true
    [ -n "$XEPHYR_PID" ] && kill "$XEPHYR_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

if [ -n "${RA2_CAPTURE_XEPHYR:-}" ] && ! DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1; then
    Xephyr "$DISPLAY_NAME" -screen 1280x1024 -ac -noreset \
        >"$OUT/xephyr.log" 2>&1 &
    XEPHYR_PID=$!
    for _ in $(seq 1 40); do
        DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1 && break
        sleep 0.25
    done
fi
DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1 ||
    die "display $DISPLAY_NAME is not live"

capture() {
    name=$1
    DISPLAY=$DISPLAY_NAME xwd -id "$WID" -silent -out "$OUT/$name.xwd"
    python3 "$XWD2PNG" "$OUT/$name.xwd" "$OUT/$name.png" --quiet >/dev/null
    if [ -n "$EXPECT" ]; then
        DISPLAY=$DISPLAY_NAME python3 "$PNGINFO" "$OUT/$name.png" \
            --label "$name" --nonuniform --expect "$EXPECT"
    else
        DISPLAY=$DISPLAY_NAME python3 "$PNGINFO" "$OUT/$name.png" \
            --label "$name" --nonuniform
    fi
}

click_scene() {
    x=$1
    y=$2
    DISPLAY=$DISPLAY_NAME python3 "$XTEST" click "$x" "$y" \
        --window-id "$WID" --scene 1280 1005 --hold 0.12 --settle 0.15
}

wait_log() {
    pattern=$1
    for _ in $(seq 1 40); do
        if grep -q -- "$pattern" "$OUT/app.log" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    echo "window_smoke: the app never logged $pattern" >&2
    echo "--- app.log ---" >&2
    tail -20 "$OUT/app.log" >&2 || true
    exit 1
}

# --- launch and find the window -------------------------------------------
DISPLAY=$DISPLAY_NAME "$APP" --game "$GAME" >"$OUT/app.log" 2>&1 &
APP_PID=$!

WID=
for _ in $(seq 1 80); do
    if FOUND=$(DISPLAY=$DISPLAY_NAME python3 "$XTEST" find --title "Open Robot Arena" 2>/dev/null); then
        WID=$(printf '%s\n' "$FOUND" | head -n 1 | awk '{print $1}')
        break
    fi
    sleep 0.25
done
[ -n "$WID" ] || die "the client did not open a window named Open Robot Arena"

GEOMETRY=$(printf '%s\n' "$FOUND" | head -n 1)
SIZE=$(printf '%s\n' "$GEOMETRY" | awk '{print $3}')
WIDTH=${SIZE%x*}
HEIGHT=${SIZE#*x}
[ -n "$EXPECT" ] || EXPECT="$SIZE"
echo "window_smoke: $WID at $SIZE on $DISPLAY_NAME, expecting $EXPECT"
DISPLAY=$DISPLAY_NAME python3 "$XTEST" focus "$WID" >/dev/null
sleep 0.6

# --- the shipped route -----------------------------------------------------
capture menu

click_scene 639.5 466.5
wait_log "exhibition/exhib_setup.gib"
sleep 0.5
capture exhibition

click_scene 80 262.89
wait_log "content exhibition/arena.gib"
sleep 0.5
capture arena

# The arena list's third visible row is Combat Arena (box.py); the headless
# --play route clicks the same point (docs/21-play.md §4).
click_scene 561.5 234.89
wait_log "arena listbox"
sleep 0.4

click_scene 888 530.97
wait_log "start game button"
sleep 3.0
capture match

# A window capture can land on the last setup frame while the match's first
# frame is being composed; retry once if the frame is still the setup screen.
MATCH_MAE=$(python3 "$ROOT/tools/oracle/compare.py" "$OUT/arena.png" "$OUT/match.png" \
    "$OUT/arena-match-diff.png" 2>/dev/null | python3 -c \
    'import json,sys; print(json.load(sys.stdin)["mae"]["all"])')
if python3 -c "import sys; sys.exit(0 if $MATCH_MAE <= 0.5 else 1)"; then
    sleep 1.5
    capture match
fi
MATCH_MAE=$(python3 "$ROOT/tools/oracle/compare.py" "$OUT/arena.png" "$OUT/match.png" \
    "$OUT/arena-match-diff.png" 2>/dev/null | python3 -c \
    'import json,sys; print(json.load(sys.stdin)["mae"]["all"])')
python3 -c "import sys; sys.exit(0 if $MATCH_MAE > 0.5 else 1)" ||
    die "the match frame is the setup frame (MAE $MATCH_MAE); START did not render"

DISPLAY=$DISPLAY_NAME python3 "$XTEST" key Escape >/dev/null
sleep 0.7
capture pause

# The pause frame must not be the match frame: a capture of the wrong moment is
# not evidence of a pause screen.
PAUSE_DIFF=$OUT/match-pause-diff.png
PAUSE_JSON=$(python3 "$ROOT/tools/oracle/compare.py" \
    "$OUT/match.png" "$OUT/pause.png" "$PAUSE_DIFF" 2>/dev/null)
printf '%s\n' "$PAUSE_JSON" >"$OUT/match-pause-diff.json"
PAUSE_MAE=$(printf '%s\n' "$PAUSE_JSON" | python3 -c \
    'import json,sys; print(json.load(sys.stdin)["mae"]["all"])')
python3 -c "import sys; sys.exit(0 if $PAUSE_MAE > 0.5 else 1)" ||
    die "Escape did not change the frame (MAE $PAUSE_MAE); pause was not captured"

# --- our offscreen menu, cropped to the scene ------------------------------
OFFSCREEN=$OUT/menu-offscreen__1280x1005.png
if [ -x "$ENGINE/target/release/examples/uishot" ]; then
    "$ENGINE/target/release/examples/uishot" \
        "$GAME/UI/mainmenu/mainmenu.gib" "$OFFSCREEN" 1280 1005 >/dev/null
else
    (cd "$ENGINE" && cargo run -q -p ra2-render --example uishot -- \
        "$GAME/UI/mainmenu/mainmenu.gib" "$OFFSCREEN" 1280 1005 >/dev/null)
fi

# The scene is 1280x1005 centred in the window; crop that rectangle.  The
# half-pixel offset at a 1280x1024 fullscreen is rounded down to row 10.
read -r CROP_X CROP_Y CROP_W CROP_H <<EOF
$(python3 - "$WIDTH" "$HEIGHT" <<'PY'
import sys
w, h = int(sys.argv[1]), int(sys.argv[2])
scale = min(w / 1280.0, h / 1005.0)
cw, ch = round(1280 * scale), round(1005 * scale)
print(round((w - cw) / 2), round((h - ch) / 2), cw, ch)
PY
)
EOF
SCENE=$OUT/menu-scene.png
if [ "$CROP_W" = 1280 ] && [ "$CROP_H" = 1005 ]; then
    python3 "$PNGCROP" "$OUT/menu.png" "$SCENE" \
        --crop "$CROP_X" "$CROP_Y" "$CROP_W" "$CROP_H" >/dev/null
else
    python3 "$PNGCROP" "$OUT/menu.png" "$SCENE" \
        --crop "$CROP_X" "$CROP_Y" "$CROP_W" "$CROP_H" --scale 1280 1005 >/dev/null
fi
MENU_DIFF=$OUT/menu-window-offscreen-diff.png
MENU_JSON=$(python3 "$ROOT/tools/oracle/compare.py" "$SCENE" "$OFFSCREEN" "$MENU_DIFF" 2>/dev/null)
printf '%s\n' "$MENU_JSON" >"$OUT/menu-window-offscreen.json"
printf '%s\n' "$MENU_JSON" | python3 -c '
import json, sys
d = json.load(sys.stdin)
pointer = d.get("pointer") or {}
masked = ((pointer.get("stats_without") or {}).get("mae") or {}).get("all")
print(
    "window menu vs offscreen: MAE %.3f (pointer-masked %s), size %s"
    % (d["mae"]["all"], "n/a" if masked is None else "%.3f" % masked, d["size"])
)
'

echo "window_smoke: ok; captures in $OUT"
