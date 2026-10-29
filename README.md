# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

*Playing it: [`docs/40-playing.md`](docs/40-playing.md) (what you need, your saves, keys, known differences).*

**M1–M10 have landed as measured, with the named residues below; M11 is a
fidelity pass (see Validation below for what was and was not measured against the original) and M12 is Linux-complete, with macOS and Windows
unverified.** The engine is a Rust workspace that reads the original `.gmf`,
`.bot`, CFL3, `.gib` and `.giff` data and runs the shipped Python 2.2 arenas and
AI on an embedded CPython 3.12. It renders with `wgpu` (window or headless) and
simulates with its own re-implementation of the original's Havok 1.7 dynamics
(Jolt Physics is an opt-in fallback behind the `jolt` cargo feature).

The client, `ra2-app`, now boots the shipped shell, opens a real match from the
exhibition screen with the players' own team/robot and game-type/arena choices,
lets the player drive it, renders the game's HUD, results (with each robot's own
snapshot) and music, builds and paints a robot in the bot lab and saves it, and
runs a nine-event season in the team HQ with points, prizes and component-level
repairs that persist. It is still not a complete replacement for the original:
the weight-class filter and the time dropdown's option list are not in the data,
the workshop's shift-mouse rotation and outline-to-mesh extrusion are not
recovered, and the event's 3D prize scene is not composed. What is unresolved
is named with its measurement rather than rounded off.

### M1 — assets render

The parsers read **181/181 shipped `.gmf` meshes**, all **57 shipped robots**
(formats 1.07, 1.08, 1.11 and 1.12) with every component resolved and placed,
and all **12 arena scripts** assembled into scenes. `ra2-render` writes a PNG
from a mesh, an assembled robot or an arena scene; `ra2 info`/`obj`/`cfz`/
`arena` inspect the data. The mount rule is the one the shipped bots and their
own previews agree on — a male plug into a female socket, `A_parent · R_z(start)
· R_x(π) · R_z(angle) · A_own⁻¹` (F1 in
[`docs/06-deferred-fidelity.md`](docs/06-deferred-fidelity.md)). Caveat: the
component *placement* fidelity items, including a burst motor's start angle,
live in that file; they are not re-measured here.

### M2 — physics sandbox

A robot drops into an arena, drives on a scripted input timeline, collides with
walls and movable props, and the run renders to a frame strip. The physics sits
behind a `PhysicsBackend` trait; Jolt was the first implementation (now the
opt-in `jolt` feature; the default is `havok17`). On the measured
45-bot box-arena survey with the game's own friction law, **37 of 45 stock robots travel 2 m or more**. Caveat:
the eight that remain are rest-pose weapons/props, two F1 wheel-height
residuals and one curved path, not the motor law or a global speed constant
(F2/F4/F5); Havok's friction-combination law (F6) is now the default.
The data is the game's: arena gravity, fixed collision geometry, per-body
friction/elasticity, and chassis mass from armour weight × surface area.

### M3 — robot fights

A robot is a **component tree** rebuilt from the game's own parts: motors turn
their attached subtree, weapons deal damage on contact, and parts whose
hitpoints run out are torn off as debris. A fight between two AI-driven robots
ends when a chassis is destroyed; the deterministic head-on ram
(`RA2_DUEL_DRIVE=ram`) opens with a **36-point blow**, and a six-second grind
shears two parts off — above the AI's own 10-point "good hit" line, around the
hazards' flat 25. Caveat: the damage scale is calibrated rather than recovered,
the chassis buckle curve is inferred, and Jolt combines friction differently
from Havok 1.7.1 (F6/F7/F9; the rest of the list is in `docs/06`).

### M4 — the script layer

The game's own Python runs unmodified: `ra2script` embeds CPython 3.12, exposes
the `plus` object model, and translates the shipped Python 2.2 as it loads. All
**14 stock arenas** register and configure themselves, the navigation map comes
from the `.map` files the arenas ship (the box arena's is 30×30 with 207 walls
and 24 hazard cells), the shipped `AI/` drives the duel on the game's own
eighth-of-a-second cadence, and standing in a registered SmartZone fires the
arena's own spikes — they hurt. Caveat: presentation is not done — camera,
audio, particle and Gooey calls are accepted and *recorded*, not played (F13).

### M5 — the game shell

Every screen is the game's own `.gib`: `ra2fmt::gib` reads all **116 shipped
screens** into widget trees, and `ra2ui` places all **4,348 widgets**;
**77 of the 116 files** place entirely on screen (the rest are fragments and
authored oversize frames, named in the test). The renderer draws art, fills,
fonts and clipped glyph runs headlessly, and the workshop's tabs are live
hit-tested clicks. `ra2-shell` boots the shipped `Scripts/Main.py`, opens the
data's main menu and walks two routes: main menu → team picker → team HQ →
robot inventory → bot lab → chassis step (7 frames), and main menu → exhibition
setup → arena picker (3 frames). Two renderers written the other way round —
`ra2-render`'s wgpu path and `tools/ui_compose.py` on the CPU — agree with each other (a self-consistency check, not fidelity to the original) to a mean
per-channel difference of **0.453/255** on `mainmenu.gib`, **0.235/255** on
`matchresults.gib` and **0.0001/255** on `statusmeters.gib`; the residue is
glyph rasterisation, not layout. Caveats: the font's record rects and page are
two packings, so
`ra2fmt` derives each glyph cell from the page's own ink runs, rotates each row
by its right-slot records and reads the advance table two records on (S8 in
[`docs/07-m5-m6-checklist.md`](docs/07-m5-m6-checklist.md)); the remaining font
residue is only titles whose authored box is narrower than the shifted advance
table, which clip as the data says. The workshop's **list and
paint tools** are the game's C++ and are named/deferred (S4), except the data
operations measured here: the paint modes and the chassis outline/mesh editor.
The window and input loop arrived with M7 (`ra2-app`).

### M6 — packaging

Linux x86_64 ships as a portable tarball with the `ra2-app` client, a
launcher, installer, `.desktop` entry and icon, built offline and
unpack-and-run smoke-tested. macOS and Windows are a documented, **unverified**
path: no build, test or artifact exists on either.

### M7 — the interactive client

`ra2-app` is the shell as an application: a `winit` window presenting the same
`ra2-render` UI pipeline, or the same loop headlessly with `--offscreen` for CI.
The shell stays 1280×1005 logical and is letterboxed into the window; a click is
mapped back through that rectangle (bar clicks are ignored), Escape quits, a
resize reconfigures the surface, and the loop ticks at the game's 50 ms. Music
follows the screen group, an activating click plays `Sounds/button_click.wav`,
and the shipped options screen edits `ra2.cfg` byte-exactly (volumes, mutes,
fullscreen, camera and the rest; the resolution dropdown is read-only because
the data carries no list). The windowed path is built but was **not** exercised
in this environment (no display); the offscreen path and its tests are what ran.
The screen-stack `Flow` and custom cursor art remain open — no cursor art ships,
and the button art the corpus names is blank.

### M8 — the match

`--match` runs the game's own match shape: the arena's start points, the 4.0 s
countdown, the HUD (`gamewindow.gib` + `statusmeters.gib` with names, points and
the three bars), the watch/static/orbit cameras, the shipped `matchresults.gib`
filled from the report, and an in-game pause dialog. The exhibition screen's
`start game button` builds that match from its own arena picker, game-type tab
and hazard checkbox, and the player drives robot 0 with W/S/A/D or the arrows
(chase camera, `C` cycles). Damage scores in every game type except King of the
Hill, whose zone points come from `king.py`; a hit under 10 damage scores
nothing, and a teammate hit scores nothing. A 30 s box DEATHMATCH now finishes
with points around `[31, 32]` rather than `[0, 0]`. The robots tab (player and
opponent are fixed), team selection and the match-time dropdown remain
unwired.

### M9 — the workshop

`--build` boots the shipped bot lab and edits a robot with the real model: a
palette click attaches one component to a mount, undo reverses it, the armour
steps set the plate (the corpus' 2× editor hardness), a library texture fills a
face group, the control board adds a control and binds it to an action from the
shipped matrix, and the preview uses the same F1 transform as the renderer and
the rig. `Save As` writes `Robot Designs/<name>.bot` with the writer and reopens
it; the scripted run prints 1 part/56.1 kg/160 hp, then 3 parts/88.6 kg/960 hp,
identical to the rig. `Test Robot` opens a real match. Baseplate drag rotation
(the shift-mouse gesture), texture-entire and the game's outline→mesh extrusion
remain open.

### M10 — the team HQ and the season

`--hq` opens the shipped `teamhq.gib` with its four tabs, the player's robots,
the league from the career's points, the season's history and its nine-event
schedule. An event is a 16-entrant single-elimination bracket: `Events.txt`'s
setting gives 2 bots × 4 rounds or 4 bots × 2 rounds, the player is match 0 slot
0, a win advances, a loss ends the event (not the season), only the final win
stores the trophy, and a season ends with the golden sawblade. Points are
`robot_points/200 + 160/rounds`; teammates never score. The career writes the
game's own `Teams/Teams.txt` and `AI/Teams.txt`, with a versioned
`Teams/career.dat` sidecar for the fields that have no recovered game home.
Repairs are a time-driven bench (`repairs.gib`), not a purchase.
`--hq --simulate-season` plays all nine events headlessly and writes the season
frames.

### M11 — fidelity

The oracle is real: the original runs under the installed flatpak Wine 9.0, and
`tools/oracle` measures our main-menu render against its captured frame at
**1.439/255** mean absolute channel difference (pointer masked); the windowed
client's own route was captured and verified with synthetic X input. The traction
survey rose from 28/45 to **37/45** after finding that Jolt was colliding each
robot's parts with their own chassis; the eight remaining cases are measured
rest-pose contacts, two F1 wheel-height residuals and one curved path. Havok's
friction-combination law is now the default (F6), the per-material damage scale
is recovered (Metal 0.1, `ramplate1` 0.4, `rubber(0)` 0.0) and applied, the
armour plates themselves are ×1.00, and the crush curve is linear in damage
with a hinge-preserving collider re-bake (F7/F9). The
match's bar geometry is the binary's; the fractions are our component hitpoints.

### M12 — release

The Linux x86_64 tarball carries `ra2-app`, the CLI tools, the launcher,
installer, `.desktop` entry and icon, built offline and smoke-tested
([`docs/08-packaging.md`](docs/08-packaging.md)). CI runs the Linux verification
steps without the game, with advisory macOS/Windows builds. macOS and Windows
are documented, **unverified** paths — scripts exist for a `.app` and a portable
zip, but neither was ever run and no artifact exists
([`docs/09-platforms.md`](docs/09-platforms.md)). Flatpak and AppImage recipes
exist and are **not built here**: `flatpak-builder`, the 24.08 SDK and
`appimagetool` are missing.

## What works / what does not

Measured, not aspirational.

**Works**

* Reading 181/181 meshes, all 57 robots, all 116 `.gib` screens, the `.giff`
  faces, CFL3 archives, team files and arena scripts.
* Rendering any shipped robot or arena to a PNG; no Wine, no Windows.
* The shell's main menu, team picker, team HQ, robot inventory, bot lab,
  chassis steps and the exhibition setup/arena picker, walked by the screens'
  own notes.
* Booting the game's own `Scripts/Main.py`; running the 14 arenas, the shipped
  AI and the hazard zones.
* A full match: spawn, countdown, HUD, cameras, pause, results, damage scoring
  and the arena's own sounds, headless and in the client; a four-slot Battle
  Royal and 2v2 Team Match, with each robot's own snapshot in the result cells.
* Building, painting, wiring, saving, reloading and testing a robot; baseplate
  placement, decal/layer paint and the chassis outline tools; the saved file
  round-trips 57/57 shipped bots byte-identically.
* A nine-event career: brackets, points, prizes, component-level repairs, the
  game's own team tables and a documented sidecar, surviving restart.
* The windowed client's own route captured with synthetic X input (menu →
  exhibition → arena → match → pause), at **7.2 ms/frame** offscreen and
  **0.057 ms/step** for the match.
* The Linux tarball with the `ra2-app` client, its launcher, and its
  unpack-and-run smoke test.

**Does not work yet**

* The exhibition's weight-class filter is not read, the time dropdown's option
  list is empty in the shipped data (`--time`/`--seconds` replace it), and the
  headless CLI names only slots 1 and 3 — slots 2/4 work in the window and take
  the roster's next robot from the CLI.
* The workshop's shift-mouse rotation, texture-entire, the game's
  outline-to-mesh extrusion/UV atlas, and the desktop's 3D preview of the
  original.
* The event's 3D prize scene (`event_results.py`), the repairs screen's 3D
  placeholder, and the match-side writer that fills `Team+0x12c`/`+0x13c` (the
  bench reads and writes them; the match inserts a report-derived list).
* Physical audio playback was not exercised. The click rule (push buttons only),
  the `Begin_*` line, weapon triggers, motor loops and the music policy are from the
  original's own sound-call log (`docs/35-audio-events.md`); open there: the
  in-match music request, `AddXtraSound`, the damage scale that sets the impact
  loudness and the crowd's excitement.
* macOS and Windows builds — documented, unverified. arm64, musl, static
  linking, Flatpak and AppImage are not built. Signing and notarisation are
  absent.
* Some fidelity residues: eight stock robots still fail the traction survey
  (rest-pose contacts, two F1 wheel-height residuals, one curved path); the
  crush curve and collider re-bake are in, but the buckle silhouette and some
  old-version mount details remain measured rather than exact (F9/F11 in
  [`docs/06-deferred-fidelity.md`](docs/06-deferred-fidelity.md)).
* Title boxes authored narrower than the shifted advance table, which clip as
  the data says. The other faces' glyph cells are the page's own runs; both
  former O fragments are whole.
* Multiplayer with the original: its DirectPlay is not compatible (the port's
  own LAN mode, `docs/38-multiplayer.md`, talks to other copies of this port
  only). Slash decals, physics fine-tuning, a gamepad, mods beyond the game's
  own directories, an update feed and localisation. The player's view of all
  this is [`docs/40-playing.md`](docs/40-playing.md).

## Run it

**New here? Start with [`docs/40-playing.md`](docs/40-playing.md)**: what you need,
how to point the port at your install, where your saves go, the keys, and what
differs from the original.

**The game is not included.** You need a Robot Arena 2 installation; point the
tools at the directory that directly contains `Robot Arena 2.exe`, `RA2.cfz`,
`UI/` and `Scripts/` (the complete install, not its parent):

```bash
export RA2_GAME_DIR="/path/to/Robot Arena 2"      # or pass --game DIR to ra2-app
```

`ra2-app` takes `--game DIR` first, then `RA2_GAME`/`RA2_GAME_DIR`, then
`local/game` above the working directory and the crate, then the usual
Windows/Wine locations (`~/.wine/drive_c/Program Files (x86)/Robot Arena 2`);
the packaged launcher adds the XDG data directory, `game/` beside itself,
`/usr/local/share`, `/usr/share` and `/opt`. A candidate is accepted only when
`RA2.cfz`, `UI/` and `Scripts/` are all present. The error says what is wrong
(no such folder, a file, the parent of the install, which names are missing)
and, with nothing found, where it looked.

### Build it

Linux x86_64, Rust stable and, on Debian/Ubuntu:

```bash
sudo apt install build-essential pkg-config python3.12-dev libasound2-dev \
                 libvulkan1 mesa-vulkan-drivers
cd engine
cargo build --release
```

The default build needs no network once the crates are fetched (`cargo build
--release --offline` works from a vendored or already-populated registry) and
no CMake: the physics (`havok17`) is plain Rust, with its own geometry queries
(`docs/29-havok17-backend.md` section 12ac), and no Jolt code is built or
linked. The binaries link CPython 3.12 (`libpython3.12.so.1.0`) at run time,
so the 3.12 runtime must be installed. The client is
`engine/target/release/ra2-app`.

The old Jolt Physics backend is an opt-in cargo feature, `--features jolt`
(`cargo build --release -p ra2-app --features jolt`, then `RA2_PHYSICS=jolt`
to run it; without the feature that setting prints a message and runs
`havok17`). It pulls in the `oxijolt` crate, whose build script downloads a
prebuilt Jolt archive with `curl` (needs the network); with
`CARGO_NET_OFFLINE=true` it instead builds the vendored Jolt sources, which
needs `cmake` and a C++ compiler (`sudo apt install cmake`).

### One command, offline

```bash
tools/check.sh
```

It runs fmt, clippy (`-D warnings`), the release test suite, a release build,
the packaging step and the unpacked-tarball smoke test, in that order, and stops
at the first failure. The smoke is the picture check: the packaged `ra2-app
--offscreen` must render the main menu byte-identically to an independent
`uishot` render of `mainmenu.gib`, one `ra2-shell` botlab route must still
write its 7 frames, the launcher must run headlessly through `RA2_GAME_DIR`,
and both missing-game errors and the CPython 3.12 check are asserted. It needs
the game (corpus tests skip without it) and prints one summary line; what it
does *not* cover is listed in [`docs/08-packaging.md`](docs/08-packaging.md) §9.
CI cannot run it for the same reason (no game in a checkout); the workflow
runs the individual steps instead.

### The headless route runner (CLI)

`ra2-shell` is still the scripted route runner; the client below is the normal
way to play.

```bash
cd engine

# Main menu -> team picker -> team HQ -> robot inventory -> bot lab -> chassis step
cargo run -q --release -p ra2-shell -- --route botlab --out /tmp/ra2-botlab

# Main menu -> exhibition setup -> the arena picker
cargo run -q --release -p ra2-shell -- --route exhibition --out /tmp/ra2-exhibition
```

A custom run is the same runner with a script (`click x y` or
`click "Widget Name"`, `wait n`, `shot name.png`):

```bash
cargo run -q --release -p ra2-shell -- --script my-input.txt --out /tmp/ra2-shell
```

The boot screen and every transition are the shipped data's. `--no-python`
skips booting `Scripts/Main.py`, not the Python dependency.

### The interactive client

```bash
cd engine
cargo run -q --release -p ra2-app -- --game ../local/game
```

That opens the shell's main menu in a window. Left-click the menu, Escape quits;
the shell is 1280×1005 logical and is letterboxed into whatever size the window
is. The same loop without a window is the CI path:

```bash
cargo run -q --release -p ra2-app -- --game ../local/game \
  --offscreen /tmp/ra2-shell.png --clicks "573.5,534.8;787.2,446.0"
```

`--clicks` is semicolon-separated points in the shell's own coordinates (the
run above is main menu → TEAM HQ → team HQ); it prints the click trace and the
final screen. `--size WxH` sets the window or PNG target, not the shell's
layout.

### Where saves go

The install passed as `--game` is never written. Careers, teams, robot designs,
custom textures and `ra2.cfg` go to a per-user save directory, in the original's
own file formats and folder layout (so saves copy in and out of the original):
`--save-dir DIR`, else `ORA_SAVE_DIR`, else `$XDG_DATA_HOME/open-robot-arena`
(`~/.local/share/open-robot-arena`) on Linux, `%APPDATA%\open-robot-arena` on
Windows, `~/Library/Application Support/open-robot-arena` on macOS. Reads look
there first and then in the install; a deleted install file is hidden by a line
in `deleted.txt`, never removed. See `docs/39-save-location.md`.

### The other modes

```bash
cd engine

# Play from the shipped exhibition screen: pick an arena, start, drive robot 0.
cargo run -q --release -p ra2-app -- --game ../local/game
#   ... in a window: click EXHIBITION -> ARENA -> a row -> START, then W/S/A/D.
#   Headless evidence with a drive timeline:
cargo run -q --release -p ra2-app -- --game ../local/game \
  --play --drive "0-2:1,0;2-4:1,1" --seconds 8 --out /tmp/ra2-play

# A match: box arena, two shipped AI bots, five named frames, a printed trace.
cargo run -q --release -p ra2-app -- --game ../local/game \
  --match --seconds 30 --out /tmp/ra2-match

# The workshop: build, paint, save to Robot Designs/, test in a match.
cargo run -q --release -p ra2-app -- --game ../local/game \
  --build --save-as "My Bot" --seconds 20 --out /tmp/ra2-build

# The team HQ: one simulated event, or the whole nine-event season.
cargo run -q --release -p ra2-app -- --game ../local/game \
  --hq --simulate-event 1 --out /tmp/ra2-hq
cargo run -q --release -p ra2-app -- --game ../local/game \
  --hq --simulate-season --out /tmp/ra2-season

# What the client would play, with no output device.
cargo run -q --release -p ra2-app -- --game ../local/game \
  --offscreen /tmp/menu.png --audio-report
```

`--start <bot>` opens the workshop from an existing design; `--arena`,
`--bots`, `--quit-at` and `--size` shape a match; `--simulate-event N` picks a
schedule row. Every mode runs headless with `--out` and in a window without it.

### The two-renderer check (self-consistency, not fidelity)

Two drawings of the same screen from the same parsed layout and glyph tables;
they agree with each other, which says nothing about the original.
The first uses wgpu, the second composites on the CPU from its own resolver and
the game's own art. `tools/ui_parity.sh` renders ten screens with both and
reports the per-screen mean absolute difference:

```bash
tools/ui_parity.sh          # needs RA2_GAME_DIR (or local/game)
```

The measured numbers are in
[`docs/05-validation-report.md`](docs/05-validation-report.md) §4.22: the
images land on the same rectangles and the remaining difference is text
rasterisation (0.0001 for `statusmeters`, 0.235 for `matchresults`, 0.453 for
`mainmenu`), not layout. The original-vs-ours measurements are in Validation below.

### The portable Linux package

```bash
tools/package.sh                # build + package + smoke (add --check for fmt/clippy/tests)
tar xzf dist/ra2-open-robot-arena-0.1.0-linux-x86_64.tar.gz
cd ra2-open-robot-arena-0.1.0-linux-x86_64
./run.sh --game "/path/to/Robot Arena 2"
```

That opens the client. The launcher also takes the headless forms:

```bash
./run.sh --game "/path/to/Robot Arena 2" --offscreen mainmenu.png
./run.sh --game "/path/to/Robot Arena 2" --match --out frames
```

The desktop entry's `Exec` is the same launcher (`Terminal=false`; the client
opens its own window). With no display, the launcher says so and points at
`--offscreen`. `./install.sh` adds a user (or system) launcher, `.desktop`
entry and icon; `--uninstall` removes them and keeps your game.

**CPython 3.12 is required and is not bundled.** `ra2-app`, `ra2-shell`,
`ra2-sim` and `ra2-duel` link `libpython3.12.so.1.0` (`ldd`/`readelf -d`), so
the loader needs the system 3.12 runtime and stdlib before `main()` runs. Check
with `ldd bin/ra2-app | grep libpython`; on Debian/Ubuntu install
`python3.12 libpython3.12`. The launcher performs this check itself, and
`run.sh --check-python` prints it.

The package ships `ra2-app` (the client), `ra2-shell` (the headless route
runner and script CLI), `ra2` (data inspector/support), `ra2-render`
(asset/scene PNGs), and the two playable headless paths `ra2-sim` and
`ra2-duel`. The game is never in the package, and the tarball is reproducible
when `SOURCE_DATE_EPOCH` is set. The tarball's `README.txt` has the player
step-by-step; `tools/smoke_package.sh` is its acceptance test.

### The other tools

In a source checkout the binaries are in `engine/target/release/` (add that
directory to `PATH`, or prefix the examples); in the unpacked tarball they are
in `bin/`. Examples (from the repository root):

```bash
# Render
ra2-render local/game/Components/mower_blade/mower_blade1.gmf out.png
ra2-render "local/game/AI/Team0/Bot0.bot" scout.png
ra2-render "local/game/Arenas/octagon.py" octagon.png

# Drive one robot around (M2)
ra2-sim "local/game/Arenas/octagon.py" "local/game/AI/Team1/Bot0.bot" out/ 6 10

# Two robots fight (M3)
ra2-duel "local/game/Arenas/octagon.py" \
  "local/game/AI/Team14/Bot2.bot" "local/game/AI/Team13/Bot1.bot" out/ 20 12
```

Linux x86_64 is the tested artifact. The macOS and modern Windows path — the
toolchain, the optional Jolt C++ build, `libpython3.12.dylib`/`python3.12.dll`, the wgpu
backend and the package shape — is in
[`docs/09-platforms.md`](docs/09-platforms.md), with what is unverified and what
would verify it. It is not a claim that those builds exist.

## Layout

```
docs/      Design notes, reverse-engineering findings, format specs
tools/     Our own analysis/extraction/build scripts + tools/check.sh
packaging/ Launcher, installer, .desktop, icon, platform scripts (unverified),
           and unbuilt Flatpak/AppImage recipes
engine/    The native engine (Rust workspace: ra2fmt, ra2-cli, ra2-render,
           ra2sim, ra2script — the script layer on an embedded CPython —,
           ra2-audio, ra2ui, the shell's layout model, ra2-shell, its headless
           route runner, and ra2-app, the interactive client)
.github/   CI: Linux fmt/clippy/tests/build/package; advisory macOS/Windows builds
dist/      (git-ignored) portable tarballs built by tools/package.sh
local/     (git-ignored) original game files and extracted data
```

## Validation

Fidelity to the original is measured only against **genuine captures of the
original game** run under Wine (`local/oracle/original/`, git-ignored), never
against our own reconstructions:

```bash
tools/oracle/fidelity.py                 # every reachable screen, ours vs original (MAE, diff, side-by-side)
tools/oracle/fidelity.py --gameplay      # + our simulation traces vs the original's live traces
```

The current numbers are in [`docs/26-fidelity-baseline.md`](docs/26-fidelity-baseline.md);
the tools that drive and record the original are in
[`tools/capture/README.md`](tools/capture/README.md) and
[`tools/oracle/README.md`](tools/oracle/README.md).

What earlier text called "measured" but was **self-referential**:

* `tools/ui_parity.sh` / `tools/ui_compose.py` and `local/parity/*__compose.png`
  (MAEs of 0.0001-0.45): both renderers use the Rust parser's glyphs and layout
  assumptions. They check that two drawings agree, not that either matches the game.
* Statements that the original cannot receive input (`docs/12-oracle.md` §3.4,
  `docs/25-windowed.md` §4) were wrong; input works through Frida.
* The only original-vs-ours number before this harness was the main menu at
  1.439/255 (rendered natively at 1024x768 by `uishot`); the shipped app path and
  every other screen were not compared. Physics/AI/damage "measured" claims were
  checked against Havok/binary constants and our own tests, not against traces of
  the original, until `--gameplay`.

## Legal

Robot Arena 2 is abandonware, no longer sold or supported by its publishers.
We are not the rights holders and do not distribute the game. The goal is
interoperability and preservation on modern platforms.
