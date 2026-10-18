# Capture and synthetic input

The tools behind `docs/25-windowed.md`. Game pixels and game files stay in
`local/` and are never committed.

## Quick start

```sh
# 1. The windowed client on a dedicated 1280x1024 Xephyr display.
Xephyr :4 -screen 1280x1024 -ac -noreset &
RA2_CAPTURE_DISPLAY=:4 RA2_CAPTURE_EXPECT=1280x1024 \
RA2_GAME=/path/to/local/game RA2_ORACLE_LOCAL=/path/to/main/local \
  tools/capture/window_smoke.sh

# 2. Our version of every captured original, at the same size.
RA2_ORACLE_LOCAL=/path/to/main/local tools/capture/ab.sh

# 3. The original menu (1024x768; use :2 and 1280x1024 for the other size).
RA2_CAPTURE_DISPLAY=:3 RA2_ORIGINAL_SIZE=1024x768 \
RA2_ORACLE_LOCAL=/path/to/main/local tools/capture/original.sh
```

`window_smoke.sh` writes `menu.png`, `exhibition.png`, `arena.png`, `match.png`
and `pause.png` plus `app.log` in `<local>/oracle/window/`. `ab.sh` writes
`<local>/oracle/ours/` and `<local>/oracle/diff/`. `original.sh` writes
`<local>/oracle/original/mainmenu__<size>__<seq>.png` and a
`click-failed.txt` next to it when the click did not change the screen.

## Individual tools

```sh
# XWD (24/32 bpp TrueColor) -> PNG, with a known sample.
tools/capture/xwd2png.py capture.xwd out.png --pixel 100,100

# XTEST input. --scene maps 1280x1005 shell coordinates through a window's
# letterbox; without it, x/y are root pixels.
tools/capture/xtest.py --display :0 windows
tools/capture/xtest.py --display :0 click 640 476 \
  --window-id 0x... --scene 1280 1005
tools/capture/xtest.py --display :0 key Escape
tools/capture/xtest.py --display :0 focus --title "Open Robot Arena"

# Frame assertions and the menu/scene crop.
tools/capture/pnginfo.py menu.png --expect 1280x1024 --nonuniform
tools/capture/pngcrop.py menu.png menu-scene.png --crop 0 10 1280 1005

# Resolve the rectangles the smoke script clicks (same resolver as the
# renderer):
RA2_GAME=/path/to/game engine/target/release/examples/uirects \
  /path/to/game/UI/mainmenu/mainmenu.gib 1280 1005 "exhibition button"
```

## A/B pointer masks

`compare.py` auto-detects the OS pointer. When a capture has the game's own
cursor in a place the scanner cannot confirm, put a sidecar next to the
reference:

```text
local/oracle/original/mainmenu__1280x1024__001.pointer   # x,y,w,h
local/oracle/original/mainmenu__1024x768__003.nopointer  # no mask; no pointer
```

`ab.sh` passes `--pointer-rect` or `--pointer-scan off` accordingly and records
the manual mask in the JSON.

## The original game's input limit

`original.sh` can reach and capture the main menu. It cannot move the original's
mouse: DirectInput8 is acquired exclusively and ignores XTEST, XWarpPointer,
XSendEvent, host-pointer input through Xephyr, `SetCursorPos`/`mouse_event` from
a Wine process (`wininput.py`), and a `/dev/uinput` virtual mouse
(`uinput_mouse.py`). The script records the failed click and never labels a
screen it did not capture. The full measured history is in `docs/25-windowed.md`
§4.
