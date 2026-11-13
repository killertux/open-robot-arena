# 40 - Playing Open Robot Arena

A short guide for a player. Open Robot Arena is a native client for
**Robot Arena 2: Design & Destroy** (2003). It does not contain the game: it
reads the data files of **your own copy**, and writes everything you change
(teams, careers, robot designs, settings) to a separate save folder, never to
your install.

## What you need

* Linux x86_64 (the tested platform; macOS and Windows are unverified, see
  `docs/09-platforms.md`).
* Your own copy of Robot Arena 2, installed (on Windows, or under Wine) and
  patched to **1.20**, the final patch: its `ReadMe.txt` starts "version 1.20".
  The port needs the folder that holds `Robot Arena 2.exe`, `RA2.cfz`, `UI/`
  and `Scripts/`, and to play also `Components/`, `Arenas/`, `AI/`, `Sounds/`
  and `Music/`: copy the whole install. The `.exe` itself is never run and
  nothing in the folder is changed. The game is not included and not
  downloaded by the port.
* No Windows or Wine needed to get that folder from the "LGU Repack"
  download (`ra2_repack.zip`, an Advanced Installer exe that already carries
  1.20): `tools/extract-install.sh ra2_repack.zip "<game folder>"` unpacks it
  with python3 alone, on macOS too. Its 1770 files were checked byte for byte
  against the same installer run under Wine. Other installers (a retail CD's
  InstallShield `data1.cab`) are not handled; install those under Wine or on
  Windows and copy the folder.
* CPython 3.12 with its shared library (`python3.12 libpython3.12` on
  Debian/Ubuntu); a Vulkan driver (Mesa's software one works) and a display.
* To build from source: a Rust toolchain (stable), a C compiler, `python3.12-dev`,
  `pkg-config`, and the ALSA development headers for audio. No internet is
  needed beyond fetching the crates (`cargo build --release --offline` once they
  are fetched), and no CMake or C++ compiler: the physics is Rust, and Jolt is
  not built unless you ask for it with `--features jolt` (that old fallback
  backend downloads a prebuilt archive, or with `CARGO_NET_OFFLINE=true` builds
  Jolt's sources, which needs `cmake` and a C++ compiler; see the README).

## Start it

```bash
cd engine
cargo build --release
./target/release/ra2-app --game "/path/to/Robot Arena 2"
```

(or `./run.sh --game "/path/to/Robot Arena 2"` from the packaged tarball.)

### Pointing it at your install

In this order: `--game DIR`, the `RA2_GAME` / `RA2_GAME_DIR` environment
variables, a `local/game` folder above the working directory, the usual
Windows and Wine locations (`~/.wine/drive_c/Program Files (x86)/Robot Arena 2`
and the Flatpak Wine prefix), and, with the packaged launcher, the XDG data
directory, `game/` beside it, `/usr/local/share`, `/usr/share` and `/opt`.

The first time you name a folder (`--game`, or the launcher's), a windowed run
remembers it in `game-dir.txt` in the save folder, so the next start (and the
desktop entry) needs no flag. There is no folder dialog: the port is a
command-line launched client, and a wrong or missing folder is reported on
stderr with the fix.

If the folder is wrong, the message says why: it does not exist, it is a file
(you can name the `.exe` itself and its folder is used), it is the parent of the
install ("The install is one level down: ..."), or files are missing (it lists
`RA2.cfz`, `UI`, `Scripts`). With no folder found it lists where it looked. A folder that boots but is
incomplete (no `Components`, `Arenas`, `AI`, `Sounds` or `Music`), or whose
`ReadMe.txt` says a version other than 1.20 (or which has no `ReadMe.txt`), starts
with a `ra2-app: warning:` line saying what is missing or which patch to apply;
the port does not refuse it, since the menus still work, but a version other
than 1.20 is unmeasured and may play differently or fail to load.

### Where your saves go

`--save-dir DIR`, else `ORA_SAVE_DIR`, else `~/.local/share/open-robot-arena`
(`%APPDATA%\open-robot-arena` on Windows, `~/Library/Application Support/...` on
macOS). The files are in the original's formats and folder layout (`Teams/`,
`Robot Designs/`, `ra2.cfg`), so they can be copied to and from the original
game. Delete the folder to start over. Resolution, full screen, volumes and
mutes persist in its `ra2.cfg` as soon as you change them in Options. Details: `docs/39-save-location.md`.

## Playing

* **Main menu**: Exhibition, Team HQ, Multiplayer, Options, Credits, Quit.
* **Exhibition**: pick a game type (Deathmatch, Battle Royal, Team Match,
  Tabletop, King of the Hill), an arena, hazards on or off, the match time, and
  the robots (yours or the computer's). Start Game; the arena intro can be
  skipped with its button or Esc; the results screen follows the match.
* **Team HQ**: New Team on the Select Team screen, then Robot Inventory
  (import a design, scrap, Workshop; the Import window's list rows show no text,
  exactly as in the original, but they can be clicked), Event Schedule (a nine-event season of
  16-robot brackets; Finish Later and Forfeit Event work), League Standings and
  Team History. The career is saved as you go and is there after a restart.
* **Workshop**: build a robot from the game's own parts, wire its controls,
  paint it and test it in a match; Save As writes a `.bot` file.
* **Options**: the shipped options window: resolution (a dropdown of the list
  the original offers), full screen, graphic detail, sound and music volume and
  mutes, the default camera. Changes are written to the save folder's
  `ra2.cfg`; Cancel restores the file. A new resolution applies the next time
  you start the game.

### Keys

| Key | Does |
| --- | --- |
| mouse | everything on the shell's screens |
| W / S / A / D or arrows | drive your robot (the robot's own wiring; a design can bind other keys) |
| C, F2, F3, F4 | camera: chase/watch/static/orbit |
| F1 | the robot's controls |
| Esc | in a match: skip the intro, pause and resume. On the menus it does nothing, as in the original |

There is **no gamepad or joystick support, and the original has none**. In
`Robot Arena 2.exe` the one `DirectInput8Create` caller (`0x5d4890`) creates
only `GUID_SysKeyboard` and `GUID_SysMouse` devices (no `EnumDevices`, no
`GUID_Joystick`, no `joyGetPos` import; `ra2.cfg`'s `Input Config` section is
empty). The wiring screen's "keyboard or joystick controls" text and the
ReadMe's "Gamepad optional" are leftovers: a robot's stored binding rows are
DirectInput keyboard scan codes. To play on a pad, map it to keys with your
system's input remapper.

## What matches the original, and what does not

Measured against the original running live (see `docs/36-playthrough.md`,
`docs/26-fidelity-baseline.md`): every screen's layout, text and art; the match
rules, scoring and end conditions; the shipped Python arenas and AI, including
the AI's path finding; damage, repairs and the career's points, prizes and
brackets; sounds and music cues; the team and robot save formats.

Known differences and open items:

* **Physics fine-tuning.** The physics is a re-implementation of the original's
  Havok 1.7 behaviour, matched to live traces but not bit-exact: a few
  stock robots still drive, tip or fight slightly differently, and a long fight
  does not repeat the original's one.
* **Slash decals.** The marks cutting weapons leave on the floor are not drawn.
* **Multiplayer.** The original used Microsoft DirectPlay; that is not
  compatible. The port has its own LAN mode (`docs/38-multiplayer.md`). The
  original running under Wine can join it (and host for it) with the
  replacement `dpnet.dll` in `tools/dpnet` plus `ora-dpnet-bridge`
  (`docs/38-multiplayer.md` §14): listing, joining, lobby chat and a match
  hosted by the original work; a match hosted by the port stops when the
  original loads its robot data.
* The workshop's shift-mouse rotation and outline-to-mesh extrusion, the
  event's 3D prize scene, and a new empty robot in the workshop starting from a
  stock chassis (the original starts with none) are open.
* macOS and Windows builds are unverified; no gamepad (nor in the original); no
  localisation.

If something blocks you, run with `--help` for the options and see the README's
troubleshooting notes.
