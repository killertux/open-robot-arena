# Oracle fixture capture

Reference frames from the **original** *Robot Arena 2*, used to A/B the shell,
workshop, HUD and result screens (M11.1). They live in `local/oracle/`, which is
git-ignored: **never commit game pixels or game files.**

## Prerequisites (this machine, verified 2026-10-17)

- Flatpak Wine 9.0: `flatpak run --command=wine org.winehq.Wine --version` →
  `wine-9.0`. No `wine` on `PATH` is needed.
- The game installed in that prefix at
  `C:\Program Files (x86)\Robot Arena 2`. If it is missing:

  ```sh
  P="$HOME/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2"
  cp -a local/game "$P"      # 738 MB; exe is byte-identical to local/game
  ```

- `Xephyr` and Python with `python-xlib` + `Pillow` (both present).
- No `Xvfb` on this machine; `Xephyr` is the nested server. A live `:0` also
  works if you are willing to run the game on the real display.

## Capture steps

1. **Back up and set the size** in the installed copy's `ra2.cfg`
   (`FullScreen=1`, and the four `FS*Width`/`FS*Height` keys):

   ```sh
   P="$HOME/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2"
   cp "$P/ra2.cfg" /tmp/ra2.cfg.bak
   sed -i -e 's/^FSDisplayWidth=.*/FSDisplayWidth=1024/' \
          -e 's/^FSDisplayHeight=.*/FSDisplayHeight=768/' \
          -e 's/^FS3DDisplayWidth=.*/FS3DDisplayWidth=1024/' \
          -e 's/^FS3DDisplayHeight=.*/FS3DDisplayHeight=768/' "$P/ra2.cfg"
   ```

2. **Start a nested screen** matching the size:

   ```sh
   Xephyr :2 -screen 1024x768 -ac -noreset &
   ```

3. **Start the game** (its working directory must be the install directory):

   ```sh
   DISPLAY=:2 flatpak run --command=sh org.winehq.Wine -c \
     'cd "$WINEPREFIX/drive_c/Program Files (x86)/Robot Arena 2" && exec wine "Robot Arena 2.exe"'
   ```

4. **Dismiss the two startup dialogs with Enter** (~+8 s “Failed to load
   font”, ~+14 s the DirectInput “not enough devices” box). The main menu is up
   at ~+24 s. Both dialogs are harmless and repeat on every launch.

5. **Drive the game by hand** to the state you want (the synthetic-click caveat
   is in `docs/12-oracle.md` §3.4). Move the pointer into a corner so the
   game's own cursor does not sit on the UI, wait one frame, then capture the
   root window:

   ```sh
   python3 -c 'import sys,Xlib.display,Xlib.X; from PIL import Image
   d=Xlib.display.Display(sys.argv[1]); r=d.screen().root
   w,h=r.get_geometry().width,r.get_geometry().height
   raw=r.get_image(0,0,w,h,Xlib.X.ZPixmap,0xffffffff)
   Image.frombytes("RGB",(w,h),raw.data,"raw","BGRX").save(sys.argv[2])' :2 frame.png
   ```

6. **Restore** `ra2.cfg` from the backup when finished.

## Fixture list and naming

Save originals as `local/oracle/original/<id>__<w>x<h>__<seq>.png`
(double underscores; `id` is lower-case and hyphenated). Ours go in
`local/oracle/ours/<id>__<w>x<h>.png` at the identical size.

| id | state | canonical size |
|---|---|---|
| `mainmenu` | main menu, first paint | 1024×768 (also capture 1280×1024) |
| `botlab-chassis-step1` | TEAM HQ → BOT LAB → Chassis → step 1 | 1024×768 |
| `exhibition-arena` | EXHIBITION → arena picker, list filled | 1024×768 |
| `match-countdown` | Combat Arena (`box`), 2 AI, DEATHMATCH, countdown | 1024×768 |
| `match-hud` | same match, ~5 s in, `watch` camera | 1024×768 |
| `match-results` | filled result screen after that match | 1024×768 |
| `koth-hud` | `hilltop`, KING OF THE HILL, bot in the zone ~10 s | 1024×768 |

**Why 1024×768:** the game supports it natively and our renderer can produce
exactly 1024×768 (`uishot <gib> out.png 1024 768`), so the comparison needs no
crop or rescale. Do **not** pair a 1280×1024 game frame with our 1280×1005
design canvas; those differ by 19 rows. The 1280×1024 menu is a
shipped-config reference only.

**Why these seven:** the menu is the shell's baseline; the bot-lab step is the
workshop's composition; the arena picker exercises the script-registered list;
countdown/HUD/results are the three match moments M8 must match; the KOTH frame
is the only visual check of `king.py`'s zone scoring.

## Manifest

Append one tab-separated row per capture to `local/oracle/manifest.tsv`:

```
id	size	file	captured	wine	renderer	state	sha256
mainmenu	1024x768	mainmenu__1024x768__000.png	2026-10-17	flatpak org.winehq.Wine 9.0	wined3d/llvmpipe	main menu, first paint	<sha256>
```

Record the exact route taken (what was clicked, in order) in `state` or a note
beside it; the paths are not scriptable yet.

## The comparison harness

Once a pair exists, fidelity is a number and a picture, not an opinion. The
paths below are from the main tree.

```sh
# render the canonical screen at the reference's size (from any checkout/worktree)
tools/oracle/render_ours.sh
#   -> local/oracle/ours/mainmenu__1024x768.png

python3 tools/oracle/compare.py \
    local/oracle/ours/mainmenu__1024x768.png \
    local/oracle/original/mainmenu__1024x768__000.png \
    local/oracle/diff/mainmenu__1024x768__diff.png --check --threshold 1.45

# render -> compare --check in one step
tools/oracle/check.sh
```

`render_ours.sh [screen.gib] [out.png] [w] [h]` defaults to
`UI/mainmenu/mainmenu.gib` at 1024×768. It finds the game through `RA2_GAME`,
`<repo>/local/game`, or the main worktree's `local/game` (a linked worktree has
no `local/` of its own), and writes into the main worktree's
`local/oracle/ours/` unless `out.png` is given.

`compare.py` requires identical dimensions (it exits 2 and prints both sizes
otherwise), and reports the same statistic as the shell's existing standard:
the mean absolute difference per RGB channel (0..255), the overall mean, the
maximum channel difference, and the fraction of pixels whose largest channel
difference exceeds `--delta` (default 8). It always prints one JSON line on
stdout; `--check` exits 1 above `--threshold` (default 0.5) and 0 below, using
the pointer-masked overall MAE when a pointer was found.

Two capture artifacts are reported, never silently cropped:

- **OS pointer.** Auto-detected as the standard dark-left-edge/bright-arrow
  shape. `pointer.rect` and `pointer.stats_without` are in the JSON; the
  rectangle is outlined in magenta on the reference panel. `--pointer-rect
  x,y,w,h` overrides it and `--pointer-scan off` disables the scan.
- **Title bar / black border.** Detected per edge; `decorations.rects` and (when
  found) `decorations.stats_without` are reported. The written image is never
  cropped.

The diff sheet is 2×2: reference (top left, pointer outlined), ours (top
right), absolute difference amplified 8× (bottom left), and a 16×16 block heat
map (bottom right; amber = block mean × 16). The full `heatmap.values` array is
in the JSON.

`check.sh` gates at the first measured main-menu number (**1.4390 → threshold
1.45**). It is not part of `tools/check.sh`: CI has no game files and no
captured originals, so this is a local, game-required gate. Without the
reference frame it exits 2 (skip), naming the missing path.

## The fidelity harness (`fidelity.py`) and the capture scripts

`tools/oracle/fidelity.py` is the comparison harness; `compare.py`/`check.sh` above are the historic
single-screen gate. Baseline numbers: `docs/26-fidelity-baseline.md`.

```sh
cd engine && cargo build --release --offline \
  && cargo build --release --offline -p ra2sim --example fidtrace
RA2_GAME=local/game python3 tools/oracle/fidelity.py                 # screens, both renderers
python3 tools/oracle/fidelity.py --only botlab,teamhq --renderer shell
python3 tools/oracle/fidelity.py --gameplay                          # + traces vs the original's live traces
python3 tools/oracle/fidelity.py --gameplay-only | --list
```

Per fixture and renderer (`app` = `ra2-app --offscreen`, `shell` = `ra2-shell` laid out natively at
1024x768) it writes `ours/`, `side/` (original | ours | |diff| x4) and `summary.md/json` under
`local/oracle/fidelity/latest/` (`--out` overrides): MAE, pointer-masked MAE, fraction of pixels >8
off, and a 16x16 low-frequency MAE. Unreachable fixtures are listed with the reason, never invented. Fixtures
and their named-click routes are the `FIXTURES` table at the top of the file; add one by adding a row (and the
capture with `add_fixture.py`). `--gameplay` runs `engine/crates/ra2sim/examples/fidtrace.rs`
(`fidtrace match|drive`, writes traces in the original's format: time, per-player location, heading
`atan2(fx,fz)`, speed, hitpoints, health) and compares per-robot speed distribution, path length,
hitpoints, damage events/s and magnitude histogram, plus the drive test (turn rate, top speed, time to 90%,
distances) when `original/drive-*.tsv` exists.

Capture scripts (all need `tools/capture/original_session.sh start`; one at a time, only one agent may run
the original):

| script | what |
|---|---|
| `capture_arenas.py [--rows 0-7]` | per stock arena: picker, intro, countdown, F2/F3/F3/F4 cameras into `original/arena-<id>-*` + manifest |
| `capture_drive.py` | in-game Python neuters both AI brains, drives Scout (turn / coast / throttle / stop), samples ~13 Hz incl. the commanded values; writes `original/drive-*.tsv/.json` |
| `capture_weapon.py` | inert Scout vs ALARM's own AI with the damage hook and a sampler with per-component health |
| `capture_repro.py --pin none|seed --runs N` | reproducibility experiment (see `docs/26`); `frida/ra2_seed.js` pins the C `rand()` |

Gotchas learnt the hard way: loading a match takes 5-20 s and running in-game Python during the load can leave a
black screen (wait blind, then check `plus.getPlayers()`); F1 in a match opens the controls-reminder overlay,
which swallows later key presses and the Continue click (use F2/F3/F4 only); a client killed mid-RPC wedges
frida-server (`kill -9 $(pgrep -x frida-server)`, restart the session); Python 2.2 inside the game rejects a
`try/except: break` in a nested `for` under the wrapper (use list comprehensions); the Select Computer Robot team
drop-down does not open with injected clicks.
