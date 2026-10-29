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

## The original game's input limit (solved 2026-10-18 with Frida)

`original.sh` alone still cannot click: X-side mouse input never reaches the
original's menus.  Measured with Frida: the X click *does* arrive as
`WM_LBUTTONDOWN`/`UP`, but the game ignores window mouse messages; its UI reads
a mouse-state object at `0x716428+0x14` (x, y, button and per-frame "moved" /
"changed" flags, layout as written by `FUN_004e6470`) inside the per-frame UI
update `FUN_005e4e30`, and under this Wine/Xephyr setup nothing ever fills it
(the "moved" byte stays 0, so even the DirectInput mouse poll `FUN_005d66c0`
is skipped).  The keyboard is fine: XTEST keys reach the DirectInput keyboard
(F1 orbit, F2/F3 static, F4 watch camera in a match).

The working path writes that object from a Frida agent, once per UI frame:

```sh
tools/capture/original_session.sh start     # Xephyr :5, game, in-sandbox frida-server
PY=~/tools/frida-venv/bin/python3
$PY tools/oracle/ra2_drive.py click 514 346 wait 3 hide shot /tmp/exhibition.png
$PY tools/oracle/ra2_drive.py -f tools/oracle/routes/exhibition_box_scout_alarm.steps
$PY tools/oracle/ra2_drive.py click 936 647 hide          # Start Game
DISPLAY=:5 tools/capture/xtest.py key F3                  # Static 3 camera
$PY tools/oracle/ra2_python.py -c 'import plus; out.write(repr(plus.getPlayers()))'
$PY tools/oracle/ra2_python.py --sample "repr([plus.getLocation(p) for p in plus.getPlayers()])" \
    --every 10 --seconds 20 > trace.tsv
$PY tools/oracle/ra2_hook.py tools/oracle/frida/ra2_damage.js --seconds 60 > damage.jsonl
tools/oracle/add_fixture.py match-hud /tmp/x.png "route / state"   # -> local/oracle/original + manifest
tools/capture/original_session.sh stop      # kills everything, restores the prefix ra2.cfg
```

| file | what |
|---|---|
| `original_session.sh` | `start`/`stop`/`status`: backs up and sizes the prefix `ra2.cfg`, starts Xephyr and the game, dismisses the two dialogs, starts `frida-server` inside the flatpak sandbox (`flatpak enter`), waits until the UI loop runs |
| `../oracle/frida/ra2_input.js` | agent: pins the UI cursor, queues button edges per UI frame (`click`, `setpos`), can also overwrite the dinput mouse state |
| `../oracle/ra2_drive.py` | step runner: `click X Y`, `move`, `hide` (cursor off-screen), `wait`, `frames`, `shot PATH`, `key KEYSYM`, `eval JS`; `-f FILE` reads steps |
| `../oracle/routes/*.steps` | recorded click routes (1024x768 game pixels) |
| `../oracle/frida/ra2_python.js`, `../oracle/ra2_python.py` | run Python 2.2 inside the game (`PyRun_SimpleString` from python22.dll, on the main thread at a UI frame): the whole `plus` API live (`getLocation`, `getHitpoints(p,0)`, `getHealth(p,comp)`, `getTimeElapsed`, ...) |
| `../oracle/frida/ra2_damage.js`, `../oracle/ra2_hook.py` | log every call to the damage sink `FUN_004175b0` (attacker, victim, component, amount, location, normal) as JSON lines |
| `../oracle/add_fixture.py` | copy a PNG or a frame directory into `local/oracle/original/` with the next sequence number and append the manifest row |

Notes: host-side `frida -p` fails with `unable to perform ptrace pokedata:
Input/output error` (the injector resolves libc from the host, not the flatpak
runtime), which is why the server runs inside the sandbox; the game is pid 2
there.  If a client is killed mid-RPC, frida-server can wedge and later
attaches hang: `kill -9 $(pgrep -x frida-server)` and start it again
(`original_session.sh stop`/`start`).  Some menus repaint late: give a few
seconds after a dialog's OK before a `shot`.  Matches are **not** reproducible
across runs (same route, three matches, two different winners); UI screens are (a re-boot
re-capture of `teamhq-select` was byte-identical).

## Audio of the original

```sh
tools/capture/record_audio.sh local/oracle/original/audio/mainmenu-music__15s__000.wav 15
```

Records only the game's own PipeWire/Pulse stream (`parec --monitor-stream=<index of "Robot Arena 2.exe">`,
48 kHz stereo s16), so host audio is not mixed in. Works with the Wine flatpak running; verified for the main
menu music and a match.

## Correction: input to the original is possible

The earlier statement that no synthetic mouse reaches the original was wrong; see "The original game's
input limit" above and `docs/12-oracle.md` §3.4. The measurement scripts built on it are in
`tools/oracle/README.md` (fidelity harness and `capture_*.py`).
