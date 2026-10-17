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

### 3.4 The one thing that did not work

With no window manager, mouse **motion** reaches the menu (the hovered button
changes art) but a synthetic XTEST **click does not activate it**; Enter also
does nothing on the menu. A retry with `metacity` did not get past the font
dialog (focus routing), and its root capture came out blank. So: **capture of
non-boot screens is a human-driven step at the display for now**; input
automation is not solved and is not needed to retire W4. Do not build a
click-driving harness until it is re-measured.

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
- `tools/oracle/compare.py` (M11.1, not written yet) loads a pair, asserts
  identical dimensions, and reports **mean absolute per-channel difference**
  over RGB, plus a difference image. Thresholds: the shell's existing
  **0.24–0.42 of 255**; match frames get a number from their first pass, not
  invented here. A corpus test that needs fixtures **skips** when
  `local/oracle/original/` is absent, exactly as the game-corpus tests do.
- `local/` is already ignored (`/local/` in `.gitignore`), so
  `local/oracle/` needs no new rule and no game pixel is ever committed.

The hand-off procedure is in [`tools/oracle/README.md`](../tools/oracle/README.md).
