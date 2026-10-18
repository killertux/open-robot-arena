# 22 — The windowed client, captured

**Measured 2026-10-18.** The client's windowed path is now exercised, not just
compiled: synthetic X pointer events click the shipped main menu, exhibition
setup, arena picker and Start Game, and the resulting frames are captured with
`xwd` and decoded by the standard-library converter. The original game's main
menu is captured at both shipped sizes. Its post-menu screens are **not**
captured, because this Wine build's DirectInput8 exclusive mouse ignores every
synthetic input path available here; §4 records exactly what was tried.

All captures live under the ignored `local/oracle/` (game pixels are never
committed). The commands below are shown from the repository root; from a
linked worktree they work the same, and `RA2_ORACLE_LOCAL` points at the main
worktree's `local/` where the game and fixtures live.

## 1. The harness

| tool | what it does |
|---|---|
| `tools/capture/xwd2png.py` | Parse a TrueColor XWD (24/32 bpp, BGRX/BGR, `bytes_per_line`) and write an 8-bit RGB PNG with `zlib`/`struct`. `--pixel X,Y` prints a known sample. Standard library only. |
| `tools/capture/xtest.py` | ctypes on `libX11`/`libXtst`: `move`, `click`, `key`, `focus`, find a window by title, `--display`, `--scene W H` mapping through the client's letterbox, plus `warp`, `move-rel`, `button`, `raise`/`lower` for the investigations. |
| `tools/capture/uinput_mouse.py` | A virtual evdev relative mouse through `/dev/uinput`, created before Wine starts. The original-game experiment; it does not reach RA2's dinput mouse. |
| `tools/capture/wininput.py` | Builds a minimal PE32 that calls `FindWindowA`, `SetForegroundWindow`, `SetCursorPos` and `mouse_event`, and runs it under the flatpak Wine. The original-game experiment; the calls succeed and the game still does not move. |
| `tools/capture/pnginfo.py` | Size, mean, stdev, range and a distinct-colour count; `--expect WxH --nonuniform` are the smoke assertions. |
| `tools/capture/pngcrop.py` | Crop a PNG (and nearest-neighbour scale) to remove fullscreen letterbox bars; uses `tools/oracle/compare.py`'s decoder. |
| `tools/capture/summarize_diff.py` | Turn a `compare.py` JSON record into the A/B columns (MAE, pointer-masked MAE, blocks > 20, largest blocks). |
| `tools/capture/window_smoke.sh` | Launch the windowed client, click the shipped route, capture `menu`/`exhibition`/`arena`/`match`/`pause`, assert size/non-uniformity, compare the menu with `uishot`. |
| `tools/capture/original.sh` | Launch the flatpak Wine original on Xephyr, dismiss its dialogs, capture the menu at 1024×768 or 1280×1024, attempt one click, and record whether it changed the screen. |
| `tools/capture/ab.sh` | Render ours at each original's size and run `tools/oracle/compare.py`, writing JSON/diff sheets and the table rows. |
| `engine/crates/ra2-app/examples/perf.rs` | Time the offscreen frame path and ten match steps. |
| `engine/crates/ra2-render/examples/uirects.rs` | Print the resolved widget rectangles, so the smoke script clicks the layout the renderer actually drew. |

The menu route was calibrated with `uirects` at the shell's 1280×1005 design
size: EXHIBITION `(639.5,466.5)`, Arena `(80,262.89)`, the arena row
`(561.5,234.89)`, Start Game `(888,530.97)`. At fullscreen 1280×1024 the client
letterboxes the 1005-row scene by 9.5 rows; `xtest.py --scene 1280 1005` maps
the point through the same `fit` the renderer and the click handler use.

## 2. Windowed verification

Run on a dedicated Xephyr display (1280×1024, no window manager) so the run does
not disturb the live desktop:

```sh
Xephyr :4 -screen 1280x1024 -ac -noreset &
RA2_CAPTURE_DISPLAY=:4 RA2_CAPTURE_EXPECT=1280x1024 \
RA2_GAME=/home/bruno/Projects/open-robot-arena/local/game \
RA2_ORACLE_LOCAL=/home/bruno/Projects/open-robot-arena/local \
  tools/capture/window_smoke.sh
```

Result (2026-10-18, 1280×1024, app `engine/target/release/ra2-app`):

```text
window_smoke: 0x00200002 at 1280x1024 on :4, expecting 1280x1024
menu: 1280x1024, mean 74.11, stdev 48.75, range 0..255, colours 4097+
exhibition: 1280x1024, mean 15.27, stdev 31.19, range 0..255, colours 4097+
arena: 1280x1024, mean 15.86, stdev 33.77, range 0..255, colours 4097+
match: 1280x1024, mean 47.46, stdev 34.06, range 0..255, colours 4097+
pause: 1280x1024, mean 49.12, stdev 34.63, range 0..255, colours 4097+
window menu vs offscreen: MAE 2.052 (pointer-masked n/a), size [1280, 1005]
```

Every frame is 1280×1024 and every one is non-uniform. The app's own log is the
proof that each click hit, not just that a pointer event was sent:

```text
click 640 466.5 -> exhibition/exhib_setup.gib [Button "exhibition button"] ...
click 80 262.5  -> exhibition/exhib_setup.gib [CheckBox "arena button"] ... content exhibition/arena.gib
click 562 234.5 -> exhibition/exhib_setup.gib [ListBox "arena listbox"] ...
click 888 530.5 -> exhibition/exhib_setup.gib [Button "start game button"] ... content exhibition/arena.gib
```

The Start Game click builds the match (`Combat Arena`, `box.py`, C.R.U.S.H. vs
Roly Poly, `camera chase`), and the captured `match.png` shows the HUD and the
`Time 0:06` clock. Escape then captures `pause.png` with the shipped
`common/ingame_dialog.gib` (`Resume Game`, `End Match`, `Options`, `Quit`); the
smoke script asserts the match/pause MAE is non-zero so a setup frame cannot be
mistaken for a pause.

**Caveats, measured:**

- The match `3D` view is a software-rendered, very close chase frame on Xephyr
  (`vulkan: No DRI3 support detected`); it is a real frame, not a designed
  screenshot. The HUD and dialog bindings are the evidence here; frame rate and
  physics timing stay with headless sims (`docs/12` §4).
- `match.png` can land on the last setup frame while the first match frame is
  composed (observed once). The script captures again after 1.5 s and fails if
  the frame is still the setup screen.
- The exhibition/arena screens are correctly composed but their `.gib`
  background is 640×480 and does not tile the 1280-wide window, so the right and
  bottom are black. That is the client's current screen art, not a capture
  failure; the menu is the frame used for the oracle comparison.

The menu comparison removes the 9.5-row letterbox first: rows 10…1014 of the
1280×1024 capture are the 1280×1005 scene. `uishot` renders the same scene
offscreen. The result is **MAE 2.052** (no OS pointer in the window capture, so
pointer masking is a no-op). The residual is the same class as the documented
original-vs-ours number: button chrome, glyph advances and the one-pixel logo
shift, not a capture defect. The crop is also the parser check: window pixel
`(100,100) = rgb(32,32,31)` and offscreen pixel `(100,90) = rgb(32,32,31)`.

## 3. `xwd2png` on our own window

The importer was exercised on a real capture, not a fixture:

```sh
DISPLAY=:4 xwd -id 0x00200002 -silent -out menu.xwd
python3 tools/capture/xwd2png.py menu.xwd menu.png --pixel 100,100
# menu.xwd: 1280x1024, depth 24, 32 bpp, 5120 bytes/line,
#           masks ff0000/00ff00/0000ff, LSB first
#   pixel (100,100) = rgb(32,32,31)
```

`menu.png` is 1280×1024, and the known sample matches the offscreen render at
the same scene point to the byte. The original game's 24-bpp dumps
(`1024x768, 24 bpp, 4096 bytes/line`) parse with the same code; only the 32-bpp
BGRX path is used for our window and the 1280×1024 original.

## 4. The original's screens: what is and is not captured

**Captured** (ignored `local/oracle/original/`):

| id | size | file | note |
|---|---|---|---|
| `mainmenu` | 1024×768 | `mainmenu__1024x768__003.png` | `tools/capture/original.sh`, pointer in the top-right corner, no in-game cursor visible |
| `mainmenu` | 1280×1024 | `mainmenu__1280x1024__001.png` | the shipped `ra2.cfg` fullscreen reference; in-game cursor at the top-left |

Both are true boot frames: the two startup dialogs were dismissed with Return,
the music was playing, and the screens match the existing deterministic
`mainmenu__*__000.png` fixtures. The 1024×768 frame is clean (no cursor); the
1280×1024 frame has the game's own cursor at `(0,19)…(10,33)`, which is why its
A/B row uses a manual 12×18 pointer mask.

**Not captured:** `botlab-chassis-step1`, `exhibition-arena`,
`match-countdown`, `match-hud`, `match-results`, `koth-hud`. No screen after the
main menu was reached, so none is claimed. The click route was attempted at the
known menu coordinates and the result is recorded in
`mainmenu__1024x768__003.click-failed.txt`: the two captures are byte-identical
(MAE 0.0).

### What was tried for the original's mouse

The startup dialogs are dismissed by **XTEST keyboard** events, which reach the
game through Wine's normal keyboard path. The mouse is different. DirectInput8
is opened with `SetCooperativeLevel(..., flags 0x5)` = **exclusive |
foreground**, and the game polls it every frame. `WINEDEBUG=+dinput` shows:

```text
dinput_device_SetCooperativeLevel hwnd 0001005C, flags 0x5
dinput_device_Acquire ...
dinput_device_GetDeviceData ... Returning 0 events queued
```

The following were tried, in order, and **none** moved the game's cursor or
activated a button:

1. **XTEST** `XTestFakeMotionEvent` / `XTestFakeRelativeMotionEvent` /
   `XTestFakeButtonEvent`, after `XSetInputFocus` and `XRaiseWindow`, and with
   the X pointer confirmed over the game's window. Motion reaches the X server;
   dinput still queues 0 events. This is the documented 2026-10-17 result,
   re-measured.
2. **`XWarpPointer`** and **`XSendEvent`** with `MotionNotify`, `ButtonPress`
   and `ButtonRelease` sent to the game window (both `send_event` and direct
   client delivery). No change; dinput still queues 0.
3. **`XTestFakeDevice*`** against the Xephyr virtual-mouse slave device. The
   client library's device-event encoding produced `BadLength` for this server,
   and the core events did not move dinput either.
4. **The host pointer through Xephyr.** `:3`'s Xephyr virtual mouse did receive
   the host motion (`xinput test` printed `motion a[0]=578 a[1]=396`), and the
   nested pointer moved, but Wine's exclusive dinput mouse ignored it as well.
5. **Wine-side input.** `tools/capture/wininput.py` builds a PE32 whose
   `FindWindowA`, `SetForegroundWindow`, `SetCursorPos` and `mouse_event` calls
   succeed (confirmed with `WINEDEBUG=+relay`: `SetCursorPos` and both
   `mouse_event` calls return 1). The game's cursor did not move;
   `wininput`'s own re-check after the game exited shows `SetCursorPos` works
   when no exclusive dinput grab is held, so the grab is what blocks it.
6. **A real evdev device.** `/dev/uinput` is writable by this user via an ACL.
   `tools/capture/uinput_mouse.py` created `ra2-capture-mouse` before Wine
   started; X saw it (`xinput` id 17, events received in `xinput test`), but
   dinput's exclusive mouse still returned 0 events. The device is unrelated to
   which X display the game uses.
7. **A window manager, to give Wine a foreground window.** Metacity on the
   nested display made the game's fullscreen D3D surface draw a blank frame
   (both fullscreen and windowed), and did not make dinput accept clicks.
   Windowed mode without a WM also rendered blank under llvmpipe.
8. **The screensaver/focus state.** The host session's screensaver was active
   and its fullscreen backup-locker window initially sat over Xephyr; dismissing
   it with `cinnamon-screensaver-command`/DBus made host pointer events reach
   Xephyr, but dinput still ignored them.

The conclusion is specific to this environment: **Wine 9.0's DirectInput8
exclusive mouse does not accept synthetic input from XTEST, XSendEvent,
XWarpPointer, the host pointer, SetCursorPos/mouse_event, or a uinput device**;
only a physical device event appears to feed it. Therefore the original's
non-boot screens remain a human-driven capture step, exactly as `docs/12` §3.4
said, now with a measured reason rather than a guess. `original.sh` captures the
menu and records the failed click without pretending otherwise.

## 5. A/B measurement

`tools/capture/ab.sh` rendered ours with `uishot` at each original's size and
ran `tools/oracle/compare.py`; JSON and diff sheets are in
`local/oracle/diff/`.  A manual pointer mask is recorded in a sidecar next to
the reference (`<ref>.pointer` with `x,y,w,h`, or `<ref>.nopointer` to disable
the auto scan); the 1280×1024 cursor uses `0,17,12,18`, and the clean 1024×768
capture has `.nopointer`.

| screen | size | MAE | pointer-masked MAE | differing regions (largest 16×16 blocks) |
|---|---|---|---|---|
| `mainmenu` | 1280×1024 | 0.8822 | 0.8673 | `(0,16)=74.7` is the game cursor at the top-left (manual 12×18 mask); then `(592,544)=65.4`, `(608,544)=64.0`, `(640,544)=63.3`, `(656,544)=62.2` — the EXHIBITION label row; 65 blocks > 20 |
| `mainmenu` | 1024×768 | 1.4460 | n/a (no pointer in frame; canonical `__000` pair: 1.4390) | `(464,416)=65.4`, `(480,416)=64.0`, `(512,416)=63.3`, `(528,416)=62.2`, `(496,416)=59.7` — the TEAM HQ label row; 72 blocks > 20 |

The 1024×768 number is the canonical pair. The **new** `__003` capture has no
OS pointer, so masking is a no-op; the pre-existing `__000` fixture has the
pointer at `(510,383)` and measures **1.4506 raw / 1.4390 pointer-masked**, which
is the number `docs/12` gates on. The two clean/pointer variants agree on the
differing regions and on the ~1.44 level; the small difference is the cursor
rectangle plus the newer capture's corner. The 1280×1024 pair is the
shipped-config reference, but **it is not a canonical A/B size**: our scene is
1280×1005 and the game's is 1280×1024, so the last 19 rows are not a
like-for-like layout. The low 0.88 is therefore a reference number, not a
regression target. The remaining differences are the same categories `docs/12`
§7 names: the logo is one pixel low in ours, the three menu button labels have
different advances and missing 1 px chrome, and the bottom Options/Credits/Quit
row differs in height and fill.

## 6. Performance

`engine/crates/ra2-app/examples/perf.rs` (release):

```text
adapter:         NVIDIA GeForce MX350 (Vulkan, DiscreteGpu)
offscreen frame: 7.2 ms/frame median (139 frames/s), 1280x1005
                 range 7.17–7.30 ms over five runs
match step:      0.057 ms/step median (~17,500 steps/s), 10 steps
                 range 0.050–0.070 ms
```

The renderer is wgpu 30; the adapter is the **NVIDIA GeForce MX350 on Vulkan**
(DiscreteGpu), selected by `PowerPreference::HighPerformance`. Forcing
`WGPU_BACKEND=gl` also works and is slower (14.2 ms/frame in a spot check);
`vulkaninfo` on this machine also reports an Intel Iris Xe and the `llvmpipe`
software adapter. The **windowed** runs on Xephyr log
`vulkan: No DRI3 support detected - required for presentation` and present
through the software path, which is why the window captures are stills rather
than a frame-rate claim. Ten match steps at 0.057 ms are the simulation's fixed
50 ms budget spent at ~0.1% — again a stills/headless number, not a physics
timing oracle (`docs/12` §4).

## 7. What remains impossible here

- **Original post-menu screens by automation.** No synthetic mouse path reaches
  DirectInput8 exclusive on this Wine build (§4). A physical pointer at the
  display, or a Wine build whose dinput accepts injected events, is required.
- **Original windowed rendering.** Fullscreen under a window manager (Metacity)
  draws a blank D3D surface under llvmpipe; windowed mode is blank too. The
  original captures therefore stay fullscreen, 1024×768 and 1280×1024.
- **A 1280×1005 original frame.** The shipped fullscreen config is 1280×1024,
  and the game has no 1280×1005 mode; the canonical like-for-like A/B stays
  1024×768.
- **Frame rate / physics timing from the original.** llvmpipe stills are fine;
  they are not evidence for the 50 ms update or for frame pacing.
