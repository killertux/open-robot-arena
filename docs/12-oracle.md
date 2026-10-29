# 12 — The oracle (W4)

**Status: settled 2026-10-17.** The original *Robot Arena 2* **does run on this
machine**, under the flatpak **Wine 9.0** that was already installed, and its
frames can be captured here at 1024×768 and 1280×1024. W4 is retired: there is
no need to borrow a Windows machine for pixels. Two limits are named in §3.4
(input automation) and §4 (timing is not an oracle). The fixture set and the
comparison contract are in §5–§6.

## 1. What exists

| Thing | State on this machine |
|---|---|
| `wine` on `PATH` | **absent** (`wine`, `wine64`, `wine-stable`, `wine-staging`, `winetricks` all missing) |
| Flatpak Wine | **`org.winehq.Wine` 9.0**, `stable-23.08`, 393.4 MB, Flathub; `flatpak run --command=wine org.winehq.Wine --version` → `wine-9.0` |
| 32-bit support | `/lib/ld-linux.so.2` present; `dpkg --print-foreign-architectures` → `i386`; `libgl1:i386`, `libvulkan1:i386` installed; runtime has `Compat.i386` and `GL32` |
| Wine prefix | `~/.var/app/org.winehq.Wine/data/wine` (`WINEPREFIX=/var/data/wine` in the sandbox) |
| Game in prefix | **already installed** at `C:\Program Files (x86)\Robot Arena 2`; `cmp` says the exe is byte-identical to `local/game/Robot Arena 2.exe`; 738 MB |
| Steam / Proton | `~/.steam` exists, **no Proton binary found**; no other Steam library with RA2 |
| Lutris / Bottles / PlayOnLinux | absent |
| `flatpak` | present; list includes `org.winehq.Wine` only (no Xvfb or other runner) |
| Display | `DISPLAY=:0`, 1920×1080, X11 session live; **Xvfb absent**; `Xephyr`, `Xorg`, `Xwayland` present; `metacity` present |
| Vulkan / GL | `vulkaninfo` 1.3.275; Mesa 25.2.8; renderer is **llvmpipe** (software) |
| `sudo -n true` | **fails** (`a password is required`) — no unattended apt install |

The game directory ships `D3D8.dll`, `D3D9.dll` and dgVoodoo, but this run did
**not** use them: the log's `wined3d_guess_card` proves Wine's built-in
`d3d8` → wined3d path was loaded (see §3). Forcing dgVoodoo is
`WINEDLLOVERRIDES="d3d8,d3d9=n"`, untested and not needed for stills.

## 2. Why the earlier "no Wine" note was wrong

The roadmap's W4 line checked `which wine`, which is empty. It did not check
`flatpak list`. The flatpak is a complete Wine 9.0 with a 32-bit runtime, and
it had already been used to install the game (prefix timestamps 2026-10-14).

## 3. What was tried, verbatim

### 3.1 A launch

```sh
Xephyr :2 -screen 1280x1024 -ac -noreset &
DISPLAY=:2 timeout 60 flatpak run --command=sh org.winehq.Wine -c \
  'cd "$WINEPREFIX/drive_c/Program Files (x86)/Robot Arena 2" && exec wine "Robot Arena 2.exe"'
```

The process stayed alive for the full 60 s (`EXIT=124` = timeout, **no crash**).
Key log lines:

```
0024:fixme:d3d:wined3d_guess_card No card selector available for card vendor 0000
    (using GL_RENDERER "llvmpipe (LLVM 17.0.6, 256 bits)").
0024:err:dmloader:get_system_default_gm_path Unable to find system path,
    default collection will not be available
0154:err:dmsynth:synth_sink_write_data Underrun detected ...
```

### 3.2 The startup sequence (three states, two dialogs)

1. ~6 s: a modal **“Failed to load font”** with `OK`. Enter dismisses it.
2. ~12 s: a DirectInput modal: **“There are not enough devices attached to the
   system for the number of users you entered. … changed to 0 …”** Enter dismisses it.
3. ~24 s: the **main menu**, music playing, at the configured resolution.

Both are the game's dialogs, neither is fatal, and `ra2.cfg` is not rewritten
(no `[Font Config]` section appears; mtime stays 2021).

### 3.3 Captured frames

The menu was captured twice, in two runs, and the PNGs are byte-identical
(sha256 `5e9ca975…` at 1280×1024): the boot frame is deterministic. The captures
are in `local/oracle/original/`, indexed by `local/oracle/manifest.tsv`:

| id | size | what |
|---|---|---|
| `mainmenu` | 1024×768 | first paint, canonical A/B size |
| `mainmenu` | 1280×1024 | the shipped `ra2.cfg` fullscreen size |

The capture method (works, no `Xvfb` needed):

```sh
Xephyr :3 -screen 1024x768 -ac -noreset &
# run the game as in §3.1 with DISPLAY=:3
# Enter at +8 s and +14 s; wait for the menu
python3 -c 'import sys,Xlib.display,Xlib.X; from PIL import Image
d=Xlib.display.Display(sys.argv[1]); r=d.screen().root
w,h=r.get_geometry().width,r.get_geometry().height
raw=r.get_image(0,0,w,h,Xlib.X.ZPixmap,0xffffffff)
Image.frombytes("RGB",(w,h),raw.data,"raw","BGRX").save(sys.argv[2])' :3 out.png
```

### 3.4 Input to the original (corrected 2026-10-18)

An earlier version of this section said automated input to the original was
impossible. **That was wrong.** What is true: with no window manager, X-side
mouse input (XTEST click, XSendEvent, warp, uinput) reaches the X server and the
game's window as `WM_*` messages but never moves its UI cursor or activates a
button, because the game's UI reads its own mouse-state object (written from
DirectInput) and ignores window mouse messages. The keyboard is fine (XTEST keys
reach it: F1..F4 cameras in a match, Return on the startup dialogs).

The working route writes that mouse-state object from a Frida agent running
inside the flatpak sandbox (`tools/capture/original_session.sh`,
`tools/oracle/ra2_drive.py`, `tools/oracle/frida/ra2_input.js`), and runs Python
inside the game for the `plus` API (`ra2_python.py`). With it every screen in
`local/oracle/original/` beyond the main menu was captured, as were live match
traces and damage logs. See `tools/capture/README.md`. Remaining quirks: some
screens repaint late (wait a few seconds before a shot), and a match is not
reproducible run to run.

## 4. Limits, stated

- **Software rendering.** llvmpipe is fine for stills; it is **not** evidence
  for frame rate, physics timing or the fixed 50 ms update. Those stay with
  headless sims and the reference machine.
- **dgVoodoo untested.** Wine's own D3D8 worked; a dgVoodoo A/B is a later
  option, not a prerequisite.
- **1280×1005 vs 1280×1024.** Our shell renderer's design canvas is 1280×1005;
  the game's 1280×1024 fullscreen has 19 more rows. **Do not compare those two
  sizes directly.** The canonical fixtures are 1024×768, where `uishot` and the
  game can produce the same dimensions with no crop. Fixtures are local only.

## 5. The fixture set to capture

Canonical: **PNG, RGB, 1024×768**, pointer in a corner (the game draws its own
cursor), one file per state, no crop. The 1280×1024 menu is the shipped-config reference.

| id | screen / moment | why it is worth a fixture |
|---|---|---|
| `mainmenu` | boot, first paint | the shell's baseline; the existing 0.24–0.42 per-channel standard |
| `botlab-chassis-step1` | TEAM HQ → BOT LAB → Chassis → structure step 1 | toolbar, content slot, the `ch_stepbuttons` overlay (M9.1, M11.5) |
| `exhibition-arena` | EXHIBITION → arena picker | the 12 script-registered arenas, cell art/text, scroll bar (M8) |
| `match-countdown` | Combat Arena (`box`), 2 AI, DEATHMATCH, countdown | spawn order/facing and the intro (M8.2, M8.4) |
| `match-hud` | same match, ~5 s into the fight, `watch` camera | HUD fields, health/points/clock, damage (M8.6) |
| `match-results` | the filled result screen after that match | `common/matchresults.gib` bindings (M8.8) |
| `koth-hud` | `hilltop`, KING OF THE HILL, ~10 s with a bot in the zone | `king.py`'s 10 points per 0.5 s (M8.3) |

Each capture records the exact route taken in `manifest.tsv` (the click paths
are not yet known precisely enough to script; write down what was clicked).

## 6. How a comparison test consumes them

- Original: `local/oracle/original/<id>__<w>x<h>__<seq>.png`.
- Ours: `local/oracle/ours/<id>__<w>x<h>.png`, rendered at the **same size**
  (e.g. `cargo run -p ra2-render --example uishot -- <screen.gib> out.png 1024 768`).
- `manifest.tsv` is the index: `id`, `size`, `file`, `captured`, `wine`,
  `renderer`, `state`, `sha256`. A pair is matched on `id` + `size`.
- `tools/oracle/compare.py` (M11.1, written; see §7) loads a pair, asserts
  identical dimensions, and reports **mean absolute per-channel difference**
  over RGB, plus a difference image. Thresholds: the shell's existing
  **0.24–0.42 of 255** is the *two-renderer* agreement (`uishot` against
  `ui_compose.py` on our own layout); an original-vs-ours pair gets its number
  from its first pass, not invented here. A corpus test that needs fixtures **skips** when
  `local/oracle/original/` is absent, exactly as the game-corpus tests do.
- `local/` is already ignored (`/local/` in `.gitignore`), so
  `local/oracle/` needs no new rule and no game pixel is ever committed.

The hand-off procedure is in [`tools/oracle/README.md`](../tools/oracle/README.md).

## 7. First measurement

Measured 2026-10-17 against `mainmenu__1024x768__000.png`, with the capture's
OS pointer and decorations reported separately. These commands are shown from
the main tree; from a worktree `tools/oracle/check.sh` is the same pair and
resolves the main worktree's ignored `local/` itself.

```sh
tools/oracle/render_ours.sh
python3 tools/oracle/compare.py \
    local/oracle/ours/mainmenu__1024x768.png \
    local/oracle/original/mainmenu__1024x768__000.png \
    local/oracle/diff/mainmenu__1024x768__diff.png --check --threshold 1.45 \
    > local/oracle/diff/mainmenu__1024x768__diff.json
# or the two steps as one gate:
tools/oracle/check.sh
```

The JSON is one line on stdout; the abridged copy below omits the paths and the
64×48 `heatmap.values` array (3072 numbers). The full line is written to
`local/oracle/diff/mainmenu__1024x768__diff.json`:

```json
{
  "size": [1024, 768], "same_size": true,
  "mae": {"r": 1.4544, "g": 1.4609, "b": 1.4363, "all": 1.4506},
  "max": 255, "delta": 8,
  "pixels_over_delta": 15377, "fraction_over_delta": 0.019553,
  "pointer": {
    "rect": [510, 383, 15, 19], "source": "auto",
    "edge_dark_run": 12, "interior_peak": 9, "masked_pixels": 285,
    "stats_without": {
      "pixels": 786147,
      "mae": {"r": 1.4429, "g": 1.4494, "b": 1.4246, "all": 1.4390},
      "max": 255, "delta": 8,
      "pixels_over_delta": 15252, "fraction_over_delta": 0.019401
    }
  },
  "decorations": {"detected": false, "title_bar": null, "black_border": null},
  "heatmap": {"block": 16, "cols": 64, "rows": 48, "scale": 16,
              "values": "<64x48 block means; see local/oracle/diff/...json>"},
  "check": {"threshold": 1.45, "metric": "mae.all.pointer_masked",
            "value": 1.4390, "passed": true}
}
```

**What the diff shows.** The art is exact apart from sub-pixel noise: the
cement backdrop is **0.000** MAE over the top-left 200×200, the left pipe
**0.131**, the panel texture **0.002**, the saw blade **0.108**, and the gear
art **0.09–0.18**; 89.5% of the 16×16 blocks are at or below 1 (73.5% exactly
zero) and only 73 of 3072 exceed 20. What differs is text and button chrome:

- The 512×76 **logo composite is one pixel lower in ours**: the purple "2"
  badge is at y 63–117 in the reference and y 64–118 in ours. Shifting ours up
  one row drops the logo crop from **8.44** to **0.51** MAE over the overlapping
  rows.
- The three **main-menu buttons** (`fg_d20`: EXHIBITION / TEAM HQ /
  MULTIPLAYER) have a 1 px black frame on the top, left and right in the
  reference and no frame in ours. Their labels also start at different x:
  reference 451 / 450 / 451, ours 463 / 470 / 451; ours is 5 px wider for
  TEAM HQ and 25 px wider for MULTIPLAYER (it ends at x 594 against 569). These
  are the brightest blocks (means 45–65 at y 336, 416 and 496).
- The **bottom bar's** Options / Credits / Quit (`fg_mcn16`) sit 3 px higher in
  ours (ink y 737–748 against 740–753) and are shorter. The reference also
  gives those buttons a 1 px light border and a lighter fill (≈ (99,99,97));
  ours has no border and a darker fill (≈ (37,37,36)). The bar art and the two
  gears match; the bottom text row still reaches block means of 47–53.

The number is higher than the **0.24–0.42** two-renderer standard because that
standard compares `uishot` with `ui_compose.py` on the same layout; this pair
compares our render with the original, so the missing button chrome and the
glyph-advance differences are included. **1.4390 is this pair's baseline**, and
it is the number `check.sh` gates on — not 0.42.

The OS pointer is present at (510,383), over the saw blade rather than in a
corner as the fixture convention asks. Excluding its 15×19 rectangle changes the
overall number by only **1.4506 → 1.4390**, and the tool always reports both.
No title bar or black border was found (Xephyr ran without a window manager); if
one appears, `compare.py` reports its rectangle and a second set of statistics
with those bands excluded, and never crops the image. The pair is the same
1024×768, so no resolution caveat applies to this number.

`tools/oracle/check.sh` gates on **1.45** (the pointer-masked 1.4390 rounded up
for compiler/renderer noise). It is deliberately **not** in `tools/check.sh`:
CI has no game files and no captured frames, so the oracle would skip there
anyway; this stays a local, game-required gate.
