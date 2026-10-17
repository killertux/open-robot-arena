# 08 — Packaging (Linux, P2)

P2's acceptance is a **portable tarball that unpacks and runs**, built and
smoke-tested on this machine. Flatpak and AppImage were **not** built: the
recipes are in `packaging/` and are marked `NOT BUILT HERE` with the reason
(no network, no toolchain). macOS and Windows are P3 and are not tested.

Everything below was measured at the P2 commit on:

| | |
|---|---|
| host | Linux Mint 22.3 (Ubuntu 24.04 base), x86_64 |
| rustc / cargo | 1.98.1 |
| CPython | 3.12.3 (`/usr/bin/python3.12`) |
| libpython | `/lib/x86_64-linux-gnu/libpython3.12.so.1.0` |
| GPU | Intel Iris Xe + NVIDIA MX350, Mesa Vulkan drivers |

No network was used: `cargo build/test/clippy` run with `--offline`, and the
package script downloads nothing.

## 1. Layout

`tools/package.sh` produces:

```
dist/ra2-open-robot-arena-0.1.0-linux-x86_64.tar.gz
dist/ra2-open-robot-arena-0.1.0-linux-x86_64.tar.gz.sha256

ra2-open-robot-arena-0.1.0-linux-x86_64/
├── bin/
│   ├── ra2-shell        the entry point (S6)
│   ├── ra2              data inspector / support tool
│   ├── ra2-render       offscreen .gmf/.bot/arena -> PNG
│   ├── ra2-sim          one-robot physics sandbox
│   └── ra2-duel         two-robot fight
├── share/
│   ├── applications/ra2-open-robot-arena.desktop
│   └── icons/hicolor/
│       ├── 256x256/apps/ra2-open-robot-arena.png
│       └── scalable/apps/ra2-open-robot-arena.svg
├── run.sh               finds the game, checks Python, runs ra2-shell
├── install.sh           --user / --system / --prefix / --uninstall
├── README.txt           the player's step-by-step guide
├── LICENSE-MIT
├── LICENSE-APACHE
├── NOTICE
└── THIRD_PARTY.md       copied from docs/THIRD_PARTY.md
```

The tarball has one top-level directory. Members are sorted, owned by
`0:0`, and stamped with `SOURCE_DATE_EPOCH` (or the build time), so setting
`SOURCE_DATE_EPOCH` makes the archive reproducible. The `.sha256` covers the
archive.

Why these five binaries:

* `ra2-shell` is the shell itself, the only program a player needs to start.
* `ra2-sim` and `ra2-duel` are the two *playable* paths today (headless):
  driving and fighting. Withholding them would leave the package with no
  game-like mode at all.
* `ra2-render` renders any shipped robot or arena to a PNG; it is the fastest
  way to confirm an install and to reproduce a bug report.
* `ra2` (`info`, `obj`, `cfz`, `arena`, `gib`, `font`) is the diagnostic tool:
  it can name a broken/missing file from a player's install, which is what
  support needs. It is 676 KB.

Total unpacked size is about 44 MiB. The binaries are not stripped: release
panic backtraces are worth more than the saved megabytes, and `ra2-render`'s
GPU stack already dominates the size. This is a documented choice, not an
oversight.

## 2. The game is not in the package

`local/` is git-ignored and the package never reads it during staging. The
smoke test asserts the unpacked tree has no `RA2.cfz`, `.gmf`, `.bot`, `.gib`
or `UI/`. The player supplies a directory containing at least:

```
RA2.cfz
UI/
Scripts/
```

`ra2-shell` needs those three to boot. The other tools additionally read
`Components/`, `Arenas/`, `AI/`, `Teams/` and the `.cfz` archives, so the
README tells the player to point at the complete install.

Discovery, in order:

1. `--game DIR` (the flag `ra2-shell` already had)
2. `RA2_GAME_DIR`
3. `$XDG_DATA_HOME/ra2-open-robot-arena/game`
   (`~/.local/share/ra2-open-robot-arena/game`)
4. `game/` beside the launcher, `../game` when installed
5. `./local/game` upwards from the current directory (source checkout)
6. `/usr/local/share/ra2-open-robot-arena/game`,
   `/usr/share/ra2-open-robot-arena/game`, `/opt/ra2/game`

A candidate is accepted only when all three required names exist. The
launcher's error lists the three names, every path it tried, and both ways to
supply the directory. The binary's own error does the same for
`--game`/`RA2_GAME_DIR`; the smoke test asserts both messages.

`run.sh --check-python` reports the launcher's CPython check and exits
non-zero if it fails. The smoke test calls it.

## 3. The Python decision (the honest one)

**Decision: require the system CPython 3.12 runtime; do not bundle it.**

The evidence is a link-time fact, not a preference:

```
$ ldd engine/target/release/ra2-shell
    libpython3.12.so.1.0 => /lib/x86_64-linux-gnu/libpython3.12.so.1.0
    ...

$ readelf -d engine/target/release/ra2-shell | grep NEEDED
 0x... (NEEDED)  Shared library: [libpython3.12.so.1.0]
 0x... (NEEDED)  Shared library: [libgcc_s.so.1]
 0x... (NEEDED)  Shared library: [libm.so.6]
 0x... (NEEDED)  Shared library: [libc.so.6]
 0x... (NEEDED)  Shared library: [ld-linux-x86-64.so.2]
```

`pyo3` with `auto-initialize` links the shared library, so the dynamic loader
resolves it **before `main()`**. Consequences, stated plainly:

* `--no-python` only skips booting `Scripts/Main.py`; it does **not** remove
  the `libpython3.12.so.1.0` dependency. There is no "no Python" build in this
  package.
* The ABI is **3.12**, not "Python 3". A host with only 3.13 cannot run these
  binaries, even if `python3` exists.
* The 3.12 **standard library** is needed too (the game's scripts import
  modules at boot).

Why not bundle CPython: it would mean shipping a second interpreter tree
(libpython + stdlib + `PYTHONHOME` wiring), chose a distro's build to trust,
and take on updating it with security fixes. The repo already documents (in
`docs/THIRD_PARTY.md`) that the interpreter is the user's. Bundling is a
separate, larger decision; it was not pretended here.

The launcher checks:

1. `ldd bin/ra2-shell` finds `libpython3.12.so.1.0` and it is not "not found";
2. a `python3.12` (or a `python3` whose `sys.version_info[:2]` is `(3, 12)`), or
   an `os.py` under `$PYTHONHOME/lib/python3.12` or the common lib dirs.

If either fails it names what is missing and gives Debian/Ubuntu, Fedora and
Arch guidance. The escape hatch for a relocatable 3.12 (e.g. pyenv on a
distro whose system Python is newer) is documented as:

```
LD_LIBRARY_PATH=/path/to/3.12/lib PYTHONHOME=/path/to/3.12 ./run.sh ...
```

Measured: on this host the launcher prints

```
CPython: python3.12 3.12.3
libpython3.12.so.1.0: found
CPython 3.12 stdlib: found
```

and the smoke test asserts all three lines. The build's `ra2script/build.rs`
symlink is a *build-time* workaround for distros that ship only the versioned
soname; it does not affect runtime and writes nothing outside `target/`.

## 4. Running and installing

Portable use (the acceptance path):

```
tar xzf dist/ra2-open-robot-arena-0.1.0-linux-x86_64.tar.gz
cd ra2-open-robot-arena-0.1.0-linux-x86_64
./run.sh --game "/path/to/Robot Arena 2"
```

With no route it runs `--route botlab --out
$XDG_STATE_HOME/ra2-open-robot-arena/frames` (default
`~/.local/state/...`) and prints every click and frame. `--route exhibition`,
`--script FILE`, `--out DIR`, `--size WxH` and `--no-python` pass through to
`ra2-shell`.

Desktop integration is optional:

```
./install.sh              # user: ~/.local/share/ra2-open-robot-arena/app,
                          #       ~/.local/bin/ra2-open-robot-arena,
                          #       XDG applications + hicolor icons
./install.sh --system     # /usr/local/{lib,bin,share}
./install.sh --prefix D   # D/{lib,bin,share}
./install.sh --uninstall  # removes the installation, never the game
```

The `.desktop` and icon are ours. `tools/make_icon.py` draws a robot head on a
workshop-blue tile with the standard library only (4x supersampled, PNG
written by hand); `packaging/ra2-open-robot-arena.svg` carries the same shapes.
No game art is read. The desktop entry uses `Terminal=true` because the shell
is headless: it prints where the frames went instead of pretending to open a
game window.

## 5. The smoke test (real, not a checklist)

```
tools/package.sh            # build + package + smoke
tools/package.sh --check    # fmt --check + clippy + cargo test + package + smoke
tools/package.sh --no-smoke # package only
tools/smoke_package.sh [tarball]
```

`tools/smoke_package.sh` unpacks into `mktemp -d`, then:

1. asserts the layout and that no game data is inside;
2. runs, from the repo root, exactly
   `bin/ra2-shell --route botlab --out DIR --game local/game`, and asserts the
   seven frames exist, are non-empty PNGs of 1280×1005, and that the log says
   `booted mainmenu/mainmenu.gib` and `finished on botlab/botlabwin.gib`;
3. renders `UI/mainmenu/mainmenu.gib` **independently** with
   `cargo run -p ra2-render --example uishot` and requires the boot frame to
   be byte-identical. This is the "boot frame is the main menu" assertion: it
   is not a hash of a remembered picture, it is a second render of the same
   data through a different entry point (the `uishot` example, not the shell).
4. runs the missing-game case against the binary and against the launcher,
   and requires the error to name `RA2.cfz`, `UI` and `Scripts`;
5. runs `run.sh` with `RA2_GAME_DIR` for the `exhibition` route and checks its
   three frames;
6. runs `run.sh --check-python` and requires the library, stdlib and a
   `python3.12 X.Y.Z` line;
7. asserts `00-mainmenu.png` differs from `01-select-team.png` (the click
   actually changed the screen).

Measured pass (this machine):

```
00-mainmenu.png        1280x1005,  66 quads / 16 textures
01-select-team.png     1280x1005,  93 quads / 16 textures
02-team-hq.png         1280x1005, 205 quads / 13 textures
03-robot-inventory.png 1280x1005, 216 quads / 18 textures
04-botlab-overview.png 1280x1005, 221 quads / 21 textures
05-botlab-chassis.png  1280x1005, 122 quads / 21 textures
06-chassis-step1.png   1280x1005, 184 quads / 23 textures
00/01/02 (exhibition)  1280x1005,  66 / 83 / 270 quads
boot frame sha256      41321481c955cc3f5c127ab6eb84a4d11a30c996d1a5ee851f063130af8eba71
reference sha256       (identical)
```

The exact hash is evidence from this machine, not a constant baked into the
test: the test compares the two files, so a renderer change moves both. (The
hash did match between the release package binary and the `--release` example
build.)

`--check` is the natural home for P4. It is present, but P4 also asks for a
one-command story for the *whole* project and is a later round; treat the flag
as the packaging half of it, not as P4 being closed.

## 6. Flatpak and AppImage: NOT built here

Both are recipes under `packaging/`, header-marked `NOT BUILT HERE`, plus
`packaging/NOT_BUILT.md` with the reason. Short version:

* no network: Flatpak needs the `org.freedesktop.Sdk` 24.08 runtime (~1 GB)
  and the Rust SDK extension; AppImage downloads its runtime and needs
  `appimagetool`/FUSE;
* neither `flatpak-builder` nor `appimagetool` is installed on this machine;
* Cargo is offline. A Flatpak sandbox starts with an empty cargo cache, so the
  manifest additionally needs `cargo vendor` — a network fetch that was not
  available.

The Flatpak manifest (`packaging/flatpak/org.openrobotarena.OpenRobotArena.yml`)
targets runtime 24.08 because it carries CPython 3.12, grants `--device=dri`
and read-only home access to the game, and installs the runner under
`/app/lib/ra2-open-robot-arena` with a symlink in `/app/bin`. The AppImage
recipe (`packaging/appimage/build-appimage.sh`) appimages the tarball and would
still link the host's libpython unless CPython 3.12 is bundled — a decision
that was not measured and is called out in the script.

Neither is evidence of anything until it is built and the smoke test is run
against the resulting artifact.

## 7. Release checklist

```
cd engine && cargo fmt --all
rm -rf ../dist
../tools/package.sh --check
```

`--check` runs `cargo fmt --all -- --check`, `cargo clippy --workspace
--offline --all-targets`, `cargo test --release --offline --workspace`, then
packages and smoke-tests. The gate for this round additionally requires the
working tree clean after commit.

Then:

1. Test the tarball on a *clean* machine (no Rust toolchain, no game in the
   repo): `./run.sh --check-python`, `./run.sh --game ...`, `./install.sh`.
2. Confirm `dist/...sha256` matches the uploaded archive.
3. Sign/attest if the project adopts that; not done here.
4. Do not upload `local/`, `downloads/`, `dist/` staging directories or any
   game file. `dist/` is git-ignored.

Version comes from `[workspace.package] version` in `engine/Cargo.toml`; the
archive name and README stamp follow it. Architecture comes from `uname -m`,
and the script refuses non-Linux hosts.

## 8. P3 placeholder — macOS and modern Windows

Not built, not tested, and this document does not claim otherwise.

**macOS.** Needs a macOS host or cross toolchain; `wgpu` on Metal; a
`libpython3.12.dylib` (3.12) and the 3.12 stdlib; a `.app` bundle with
`Info.plist`, icon and a launcher; hardened-runtime signing and notarisation
for distribution. `pyo3` must be pointed at the 3.12 framework/dylib; the
`ra2script/build.rs` symlink helper assumes ELF/Linux sonames and would need a
`dylib` branch. Architecture: test arm64 and x86_64 (or a universal binary)
separately.

**Windows.** Needs MSVC and a Windows host (or a verified cross setup); a
`python3.12.dll`/import library and stdlib; `wgpu` on DX12/Vulkan; console vs
windowed subsystem for the headless tools; a `.zip`/installer and a `.ico`.
`pyo3`'s Windows linkage and `ra2script`'s path/UNC handling are untested. The
game's own `D3D8.dll`/`dgVoodoo` must not be used or shipped.

Both need the same smoke evidence as Linux: an unpacked artifact, the shell
routes, the boot-frame comparison, the missing-game messages and the Python
check. That is deliberately not asserted here.

## 9. What this round does not cover

* **Flatpak and AppImage** — recipes only; unbuilt and unverified (section 6).
* **macOS and Windows** — P3 placeholder above.
* **A window and input.** The shell stays headless; the desktop entry launches
  a terminal run. The in-match C++ is not reimplemented; the shell stops at the
  arena picker, and `ra2-sim`/`ra2-duel` are the playable physics paths.
* **Audio/video** (no sound or movie playback).
* **Cross-distro testing.** Verified on one host (Mint 22.3 / Ubuntu 24.04,
  CPython 3.12.3). Other distros need matching 3.12 runtimes; the launcher
  says so rather than degrading.
* **arm64, musl, static linking.** Not built.
* **Bundled Python.** Deliberately not done (section 3).
* **Signing, notarisation, update feed, CI.** None.
* **Game data.** Never included; the player's own copy is required.
