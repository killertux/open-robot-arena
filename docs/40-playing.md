# 40 - Playing Open Robot Arena

A short guide for a player. Open Robot Arena is a native client for
**Robot Arena 2: Design & Destroy** (2003). It does not contain the game: it
reads the data files of **your own copy**, and writes everything you change
(teams, careers, robot designs, settings) to a separate save folder, never to
your install.

## What you need

* Linux x86_64 (the tested platform; macOS and Windows are unverified, see
  `docs/09-platforms.md`).
* Your Robot Arena 2 folder: the one that holds `Robot Arena 2.exe`,
  `RA2.cfz`, `UI/` and `Scripts/`. The `.exe` itself is never run.
* CPython 3.12 with its shared library (`python3.12 libpython3.12` on
  Debian/Ubuntu); a Vulkan driver (Mesa's software one works) and a display.
* To build from source: a Rust toolchain (stable), `cmake` and a C++ compiler
  (Jolt is built from source), `python3.12-dev`, `pkg-config`, and the ALSA
  development headers for audio. Set `CARGO_NET_OFFLINE=true` if you have no
  network: otherwise a build script tries to download a prebuilt physics
  archive (see the README).

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

If the folder is wrong, the message says why: it does not exist, it is a file
(you can name the `.exe` itself and its folder is used), it is the parent of the
install ("The install is one level down: ..."), or files are missing (it lists
`RA2.cfz`, `UI`, `Scripts`). With no folder found it lists where it looked.

### Where your saves go

`--save-dir DIR`, else `ORA_SAVE_DIR`, else `~/.local/share/open-robot-arena`
(`%APPDATA%\open-robot-arena` on Windows, `~/Library/Application Support/...` on
macOS). The files are in the original's formats and folder layout (`Teams/`,
`Robot Designs/`, `ra2.cfg`), so they can be copied to and from the original
game. Delete the folder to start over. Details: `docs/39-save-location.md`.

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

There is **no gamepad support** (the original had none we could use either:
its robots store keyboard and joystick codes, and only the keyboard is wired).

## What matches the original, and what does not

Measured against the original running live (see `docs/36-playthrough.md`,
`docs/26-fidelity-baseline.md`): every screen's layout, text and art; the match
rules, scoring and end conditions; the shipped Python arenas and AI, including
the AI's path finding; damage, repairs and the career's points, prizes and
brackets; sounds and music cues; the team and robot save formats.

Known differences and open items:

* **Physics fine-tuning.** The physics is a re-implementation of the original's
  Havok 1.7 behaviour on Jolt, matched to live traces but not bit-exact: a few
  stock robots still drive, tip or fight slightly differently, and a long fight
  does not repeat the original's one.
* **Slash decals.** The marks cutting weapons leave on the floor are not drawn.
* **Multiplayer.** The original used Microsoft DirectPlay; that is not
  compatible. The port has its own LAN mode (`docs/38-multiplayer.md`) that
  talks to other copies of this port only, never to the original.
* The workshop's shift-mouse rotation and outline-to-mesh extrusion, the
  event's 3D prize scene, and a new empty robot in the workshop starting from a
  stock chassis (the original starts with none) are open.
* macOS and Windows builds are unverified; no gamepad; no localisation.

If something blocks you, run with `--help` for the options and see the README's
troubleshooting notes.
