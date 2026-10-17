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
