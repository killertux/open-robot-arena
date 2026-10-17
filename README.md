# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

**M1–M5 have landed as measured, with the named residues below; M6 is done for
Linux x86_64; M7's interactive shell (window, input, the shared loop) is in.**
The engine is a
Rust workspace that reads the original `.gmf`, `.bot`, CFL3, `.gib` and `.giff`
data and runs the shipped Python 2.2 arenas and AI on an embedded CPython 3.12.
It renders headlessly with `wgpu` and simulates with Jolt. It is an engine and a
verification harness more than a finished game client: the shell now opens in a
window, but the in-match game is still the original C++, and audio, settings, the
bot lab's editors and the match are not playable through the shell yet.

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
behind a `PhysicsBackend` trait; Jolt is the implementation. On the measured
45-bot box-arena survey, **27 of 45 stock robots travel 2 m or more**. Caveat:
the rest creep or barely move — the wheel/contact model, the arena colliders and
Jolt's friction combination law, not a global speed constant (F2/F4/F5/F6).
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
**57 of the 116 files** place entirely on screen (the rest are fragments and
authored oversize frames, named in the test). The renderer draws art, fills,
fonts and clipped glyph runs headlessly, and the workshop's tabs are live
hit-tested clicks. `ra2-shell` boots the shipped `Scripts/Main.py`, opens the
data's main menu and walks two routes: main menu → team picker → team HQ →
robot inventory → bot lab → chassis step (7 frames), and main menu → exhibition
setup → arena picker (3 frames). Two renderers written the other way round —
`ra2-render`'s wgpu path and `tools/ui_compose.py` on the CPU — agree to a mean
per-channel difference of **0.42/255** on `mainmenu.gib` and **0.24/255** on
`hq_league.gib`. Caveats: the font's record rects and page are two packings, so
`ra2fmt` derives each glyph cell from the page's own ink runs, rotates each row
by its right-slot records and reads the advance table two records on (S8 in
[`docs/07-m5-m6-checklist.md`](docs/07-m5-m6-checklist.md)); the remaining font
residue is only titles whose authored box is narrower than the shifted advance
table, which clip as the data says. The workshop's **list and
paint tools** are the game's C++ and are named/deferred (S4). The window and
input loop arrived with M7 (`ra2-app`).

### M6 — packaging

Linux x86_64 ships as a portable tarball with the `ra2-app` client, a
launcher, installer, `.desktop` entry and icon, built offline and
unpack-and-run smoke-tested
([`docs/08-packaging.md`](docs/08-packaging.md)). macOS and Windows are a
documented, **unverified** path: no build, test or artifact exists on either
([`docs/09-platforms.md`](docs/09-platforms.md)). CI (`.github/workflows/ci.yml`)
runs the Linux verification steps without the game, plus advisory macOS and
Windows builds.

### M7 — the interactive shell

`ra2-app` is the shell as an application: a `winit` window presenting the same
`ra2-render` UI pipeline, or the same loop headlessly with `--offscreen` for CI.
The shell stays 1280×1005 logical and is letterboxed into the window; a click is
mapped back through that rectangle (bar clicks are ignored), Escape quits, a
resize reconfigures the surface, and the loop ticks at the game's 50 ms. The
windowed path is built but was **not** exercised in this environment; the
offscreen path and its tests are what ran here.
Audio, `ra2.cfg`, the screen-stack `Flow`, text entry and the cursor art
(M7.4–M7.8) are still open.

## What works / what does not

Measured, not aspirational.

**Works**

* Reading 181/181 meshes, all 57 robots, all 116 `.gib` screens, the `.giff`
  faces, CFL3 archives, team files and arena scripts.
* Rendering any shipped robot or arena to a PNG; no Wine, no Windows.
* The shell's main menu, team picker, team HQ, robot inventory, bot lab and
  chassis steps, walked by the screens' own notes; the exhibition setup and
  the arena picker with its 12 arena names.
* Booting the game's own `Scripts/Main.py`; running the 14 arenas, the shipped
  AI and the hazard zones.
* The physics sandbox and two-robot duels, headless; 27/45 stock bots reach
  2 m on the survey.
* The Linux tarball with the `ra2-app` client, its launcher, and its
  unpack-and-run smoke test.

**Does not work yet**

* The shell's window is mouse-and-Escape only: there is no keyboard focus
  navigation, no audio and no settings file, and the screen stack is still the
  runner's rather than a `Flow` shared with the game.
* The actual match (driving, damage, scoring) is the original C++ and is not
  playable through the shell — the route stops at the arena picker.
  `ra2-sim` and `ra2-duel` are the playable headless physics paths.
* The workshop's component-list and paint tools (S4), and the in-match HUD's
  live state, are not implemented.
* Audio, music and video playback.
* Some fidelity residues: a share of stock robots still lacks wheel traction,
  the damage scale is calibrated, and Jolt's friction law differs from Havok's
  (F6/F7/F9/F11 in [`docs/06-deferred-fidelity.md`](docs/06-deferred-fidelity.md)).
* Title boxes authored narrower than the shifted advance table, which clip as
  the data says. The other faces' glyph cells are the page's own runs; both
  former O fragments are whole.
* macOS and Windows builds — documented, unverified. arm64, musl, static
  linking, Flatpak and AppImage are not built.
* Game data, mod support, network play, an update feed and signing/CI.

## Run it

**The game is not included.** You need a Robot Arena 2 installation; point the
tools at the directory that directly contains `RA2.cfz`, `UI/` and `Scripts/`
(the complete install, not its parent):

```bash
export RA2_GAME_DIR="/path/to/Robot Arena 2"
```

`RA2_GAME_DIR` wins; `ra2-app` otherwise looks for `RA2_GAME`/`RA2_GAME_DIR`,
then `local/game` above the working directory and the crate, and the packaged
launcher adds the XDG data directory, `game/` beside itself,
`/usr/local/share`, `/usr/share` and `/opt`. A candidate is accepted only when
all three names are present, and the error lists the names and every path
tried.

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

### The two-renderer check

Two independent drawings of the same screen; they share no code or algorithm.
The first uses wgpu, the second composites on the CPU from the resolver's own
rectangles and the game's own art. From the repository root:

```bash
(cd engine && cargo build -q --release -p ra2ui --example uidump)
(cd engine && cargo run -q --release -p ra2-render --example uishot -- \
  "$RA2_GAME_DIR/UI/mainmenu/mainmenu.gib" /tmp/mainmenu-wgpu.png 1280 1005)
python3 tools/ui_compose.py \
  "$RA2_GAME_DIR/UI/mainmenu/mainmenu.gib" /tmp/mainmenu-cpu.png
```

Compare the two PNGs. The measured mean per-channel differences are in
[`docs/03-engine-design.md`](docs/03-engine-design.md) (M5), not hidden in this
README.

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
toolchain, the Jolt C++ build, `libpython3.12.dylib`/`python3.12.dll`, the wgpu
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

## Legal

Robot Arena 2 is abandonware, no longer sold or supported by its publishers.
We are not the rights holders and do not distribute the game. The goal is
interoperability and preservation on modern platforms.
