# 08 — Packaging (Linux, P2)

P2's acceptance is a **portable tarball that unpacks and runs**, built and
smoke-tested on this machine. Flatpak and AppImage were **not** built: their
recipes are now complete and statically checked under `packaging/`, and are
marked `NOT BUILT HERE` with the exact missing tools (section 6). macOS and
Windows are P3: they are documented in
[`09-platforms.md`](09-platforms.md) and are **not built or tested**.

Since P2 the shell gained its application (`ra2-app`, M7), so M12 repackaged
it: the tarball ships the client, the launcher and the `.desktop` entry start
it, and the smoke test checks the client's main-menu frame while keeping one
`ra2-shell` route run for the CLI. The measurements below are from this
machine.

Everything below was measured at the M12 commit on:

| | |
|---|---|
| host | Linux Mint 22.3 (Ubuntu 24.04 base), x86_64 |
| rustc / cargo | 1.98.1 |
| CPython | 3.12.3 (`/usr/bin/python3.12`) |
| libpython | `/lib/x86_64-linux-gnu/libpython3.12.so.1.0` |
| GPU | Intel Iris Xe + NVIDIA MX350, Mesa Vulkan drivers |

No network was used: `cargo build/test/clippy` run with `--offline`, and the
package script downloads nothing. `tools/package.sh`, `tools/smoke_package.sh`
and `tools/check.sh` also export `CARGO_NET_OFFLINE=true`: `--offline` tells
Cargo, but not a Cargo *build script*, and `oxijolt-sys`'s optional `prebuilt`
archive is fetched with `curl`. With the variable set, a fresh build takes the
vendored CMake path instead.

## 1. Layout

`tools/package.sh` produces:

```
dist/ra2-open-robot-arena-0.1.0-linux-x86_64.tar.gz
dist/ra2-open-robot-arena-0.1.0-linux-x86_64.tar.gz.sha256

ra2-open-robot-arena-0.1.0-linux-x86_64/
├── bin/
│   ├── ra2-app         the client (window, or --offscreen/--match/--build/--hq)
│   ├── ra2-shell       the headless route runner / scripted runner
│   ├── ra2             data inspector / support tool
│   ├── ra2-render      offscreen .gmf/.bot/arena -> PNG
│   ├── ra2-sim         one-robot physics sandbox
│   └── ra2-duel        two-robot fight
├── share/
│   ├── applications/ra2-open-robot-arena.desktop
│   └── icons/hicolor/
│       ├── 256x256/apps/ra2-open-robot-arena.png
│       └── scalable/apps/ra2-open-robot-arena.svg
├── run.sh               finds the game, checks Python, runs ra2-app
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

Why these six binaries:

* `ra2-app` is the client: the shell in a `winit` window, or the same loop
  headlessly with `--offscreen` (one PNG), `--match`, `--build` and `--hq`
  with `--out DIR`. It links `ra2-audio` (rodio) and `ra2script` (CPython
  3.12) through the workspace, so it is the binary that carries the new
  dependencies. It is 17.7 MiB (18,562,736 bytes) unstripped.
* `ra2-shell` remains the scripted/headless route runner: the shipped botlab
  and exhibition routes, `--script`, and the frame-per-screen output. It is
  what the CLI part of the smoke test exercises.
* `ra2-sim` and `ra2-duel` are the two *playable* headless physics paths:
  driving and fighting.
* `ra2-render` renders any shipped robot or arena to a PNG; it is the fastest
  way to confirm an install and to reproduce a bug report.
* `ra2` (`info`, `obj`, `cfz`, `arena`, `gib`, `font`) is the diagnostic tool:
  it can name a broken/missing file from a player's install, which is what
  support needs. It is 713 KB.

Total unpacked size is about 62 MiB, about 22 MiB compressed. The binaries are
not stripped: release panic backtraces are worth more than the saved megabytes,
and the GPU/UI/audio stack already dominates the size. This is a documented
choice, not an oversight.

## 2. The game is not in the package

`local/` is git-ignored and the package never reads it during staging. The
smoke test asserts the unpacked tree has no `RA2.cfz`, `.gmf`, `.bot`, `.gib`
or `UI/`. The player supplies a directory containing at least:

```
RA2.cfz
UI/
Scripts/
```

`ra2-app` needs those three to boot (it boots the same shell as
`ra2-shell`). The other tools additionally read `Components/`, `Arenas/`,
`AI/`, `Teams/` and the `.cfz` archives, so the README tells the player to
point at the complete install.

Discovery, in order:

1. `--game DIR` (the flag `ra2-app` and `ra2-shell` already had)
2. `RA2_GAME_DIR` (and `RA2_GAME`, for `ra2-app`)
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

If neither `DISPLAY` nor `WAYLAND_DISPLAY` is set, the launcher refuses a
windowed run with the same wording `ra2-app` uses and points at `--offscreen`
(plus the `--match`/`--build`/`--hq` headless runs). The smoke test asserts
that message. `--offscreen PNG` and `--out DIR` are not display-gated.

## 3. The Python decision (the honest one)

**Decision: require the system CPython 3.12 runtime; do not bundle it.**

The evidence is a link-time fact, not a preference:

```
$ ldd engine/target/release/ra2-app
    libpython3.12.so.1.0 => /lib/x86_64-linux-gnu/libpython3.12.so.1.0
    ...

$ readelf -d engine/target/release/ra2-app | grep NEEDED
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

1. `ldd bin/ra2-app` finds `libpython3.12.so.1.0` and it is not "not found";
2. a `python3.12` (or a `python3` whose `sys.version_info[:2]` is `(3, 12)`), or
   an `os.py` under `$PYTHONHOME/lib/python3.12` or the common lib dirs.

If either fails it names what is missing and gives Debian/Ubuntu, Fedora and
Arch guidance. The escape hatch for a relocatable 3.12 (e.g. pyenv on a
distro whose system Python is newer) is documented as:

```
LD_LIBRARY_PATH=/path/to/3.12/lib PYTHONHOME=/path/to/3.12 ./run.sh ...
```

`--check-python` prints the same report as the smoke test; when the check
fails it also prints the install guidance above (so an AppImage's `AppRun`,
which calls this, shows the launcher's own message rather than a second copy
that could drift).

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

That opens `bin/ra2-app` at the main menu in a window; a click goes through
the letterboxed 1280×1005 shell and Escape quits. Headless runs are the same
binary:

```
./run.sh --game "/path/to/Robot Arena 2" --offscreen mainmenu.png
./run.sh --game "/path/to/Robot Arena 2" --match --out frames
./run.sh --game "/path/to/Robot Arena 2" --build --out frames
./run.sh --game "/path/to/Robot Arena 2" --hq --out frames
```

The route runner is still shipped for support and scripts; arguments are no
longer rewritten by the launcher, so call it directly:

```
bin/ra2-shell --game "/path/to/Robot Arena 2" --route botlab --out DIR
bin/ra2-shell --game "/path/to/Robot Arena 2" --route exhibition --out DIR
bin/ra2-shell --game "/path/to/Robot Arena 2" --script input.txt --out DIR
```

`--size WxH` sets the window/PNG target for `ra2-app` (the shell stays
1280×1005 logical). `--no-python` is a `ra2-shell` flag; it skips booting
`Scripts/Main.py`, not the link-time Python dependency.

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
No game art is read. The desktop entry's `Exec` is the launcher and
`Terminal=false`: the client opens its own window. Without a display it prints
the `--offscreen` hint to the journal instead.

## 5. The smoke test (real, not a checklist)

```
tools/package.sh            # build + package + smoke
tools/package.sh --check    # fmt --check + clippy + cargo test + package + smoke
tools/package.sh --no-smoke # package only
tools/smoke_package.sh [tarball]
```

`tools/smoke_package.sh` unpacks into `mktemp -d`, then:

1. asserts the layout, that `run.sh` runs `bin/ra2-app` and that the desktop
   entry's `Exec` is the launcher, and that no game data is inside;
2. runs the packaged client with no display, from the repo root, exactly
   `bin/ra2-app --game local/game --offscreen DIR/app-mainmenu.png`; the log
   must say `booted mainmenu/mainmenu.gib` and
   `finished on mainmenu/mainmenu.gib`, and the PNG must be 1280×1005;
3. renders `UI/mainmenu/mainmenu.gib` **independently** with
   `cargo run -p ra2-render --example uishot` and requires the client's frame
   to be byte-identical. This is the "boot frame is the main menu" assertion:
   it is not a hash of a remembered picture, it is a second render of the same
   data through a different entry point (the `uishot` example, not `ra2-app`,
   and not `ra2-shell`);
4. keeps the CLI honest with one route run:
   `bin/ra2-shell --route botlab --out DIR --game local/game` must write the
   seven named frames, log the boot and the workshop finish, and its first
   click must change the screen;
5. runs the missing-game case against `ra2-app` and against the launcher, and
   requires the error to name `RA2.cfz`, `UI` and `Scripts`;
6. runs the launcher with neither display variable set and asserts it refuses
   a windowed run and names `--offscreen`;
7. runs `run.sh` headlessly with `RA2_GAME_DIR` (`--offscreen`) and requires
   its PNG to equal the same independent reference;
8. runs `run.sh --check-python` and requires the library, stdlib and a
   `python3.12 X.Y.Z` line, and finally repeats the launcher's missing-game
   error.

Measured pass (this machine, M12):

```
app mainmenu          1280x1005, 66 quads / 16 textures
00-mainmenu.png       1280x1005, 66 quads / 16 textures  (ra2-shell)
01-select-team.png    1280x1005, 92 quads / 15 textures
02-team-hq.png        1280x1005, 205 quads / 13 textures
03-robot-inventory    1280x1005, 216 quads / 18 textures
04-botlab-overview    1280x1005, 221 quads / 21 textures
05-botlab-chassis     1280x1005, 122 quads / 21 textures
06-chassis-step1      1280x1005, 184 quads / 23 textures
app boot frame sha256 e74d68b60fb99ef8c7df71e9d2bbfacfe50f9fb94f677e3dc0f7fe25180aabc0
reference sha256      (identical)
launcher frame        (identical)
```

The exact hash is evidence from this machine, not a constant baked into the
test: the test compares the files, so a renderer change moves all of them.
The hash moved from P2's `4132…` because the client's frame builder and the
UI pipeline changed since; equality, not the value, is the assertion.

`--check` is the packaging half of P4. The whole-project command is
`tools/check.sh`, which reuses this smoke test rather than duplicating it; see
§9.

## 6. Flatpak and AppImage: recipes checked, NOT built here

Both recipes now exist in full and were statically checked. **Neither was
built, installed or run.** The release is still the tarball.

| | Flatpak | AppImage |
|---|---|---|
| Recipe | `packaging/flatpak/org.openrobotarena.OpenRobotArena.yml` | `packaging/appimage/build-appimage.sh` |
| Entry | `packaging/flatpak/ra2-first-run.sh` → `run.sh` | `packaging/appimage/AppRun` → `run.sh` |
| Metadata | `.desktop`, PNG/SVG icon, `…metainfo.xml` | `.desktop`, PNG/SVG icon, same metainfo |
| Python | runtime `24.08` supplies CPython 3.12 | host CPython 3.12 required, not bundled |
| State | **not built** | **not built** |
| Missing here | `flatpak-builder`; the 24.08 SDK + Rust extension; network | `appimagetool`; optional `linuxdeploy`; AppImage runtime/FUSE; network |

### 6.1 Flatpak

The manifest uses `org.freedesktop.Platform`/`org.freedesktop.Sdk` **24.08**
plus `org.freedesktop.Sdk.Extension.rust-stable`. 24.08 is the right runtime
because it carries CPython 3.12 (freedesktop-sdk 24.08.21 updated python3 to
3.12.11-0), the ABI the binaries link against. The **23.08** runtime installed
on this host was checked and has **Python 3.11.13**, which cannot satisfy the
`libpython3.12.so.1.0` dependency; the SDK half of 24.08 supplies the 3.12
headers and `python3.pc` that `pyo3` needs at build time. 24.08 itself was not
installed here, so that is a fact from the runtime's release notes, not a run.

The module builds the same workspace members `tools/package.sh` does:

```
cargo build --release --offline --manifest-path engine/Cargo.toml \
    -p ra2-app -p ra2-shell -p ra2-cli -p ra2-render -p ra2sim
```

and installs `ra2-app` plus the five tools under
`/app/lib/ra2-open-robot-arena/bin`, with symlinks for all six in `/app/bin`
(so `flatpak run --command=ra2-shell` etc. work), `run.sh` and the first-run
wrapper beside them, a symlink `/app/bin/ra2-open-robot-arena` as the default
command, the `.desktop` entry under the app id, the hicolor PNG/SVG, and the
AppStream metainfo under `/app/share/metainfo`. `local/`, `dist/`,
`downloads/`, `engine/target` and `.git` are skipped, so the player's game and
old build output never enter the sandbox.

`finish-args` are: Wayland and X11 (`--socket=wayland`,
`--socket=fallback-x11`, `--share=ipc`), PulseAudio, `--device=dri`,
`--filesystem=home:ro`, and `--env=GTK_USE_PORTAL=1`. There is deliberately
no `--share=network`: v1.0 has no network play, update check or telemetry.

**Game directory.** The game is never bundled. `home:ro` is the blunt grant:
read-only access to the whole home directory, which covers the documented
`~/.local/share/ra2-open-robot-arena/game` default and any install under
`$HOME`. It is not `--filesystem=home` (read-write) and not a static
per-path grant, because the player's path is unknown at build time. On first
run `ra2-first-run.sh` opens a directory chooser through `zenity` and the XDG
desktop portal; the chosen directory is exported by the document portal and
its path is passed to `run.sh` as `--game`, then remembered in
`$XDG_CONFIG_HOME/ra2-open-robot-arena/game-dir` (on the host, under
`~/.var/app/org.openrobotarena.OpenRobotArena/config/`). Document-portal paths
can go stale across sessions, so the wrapper validates the saved path every
run and asks again. A game outside `$HOME` is most durable with one override:

```
flatpak override --user --filesystem="/path/to/Robot Arena 2:ro" \
    org.openrobotarena.OpenRobotArena
```

**Offline Cargo.** A Flatpak sandbox starts with an empty `CARGO_HOME`, so
`--offline` alone cannot work. `packaging/flatpak/gen-cargo-sources.py` reads
`engine/Cargo.lock` and writes `cargo-sources.json` (320 crates.io archives,
320 inline `.cargo-checksum.json` files and a `cargo/config` pointing at
`cargo/vendor`). flatpak-builder extracts them; the manifest sets
`CARGO_HOME=/run/build/ra2-open-robot-arena/cargo` and
`CARGO_NET_OFFLINE=true`, which also stops `oxijolt-sys`'s build script from
fetching its optional prebuilt Jolt archive, so Jolt 5.6.0 is built from the
vendored CMake sources. Regenerate with `gen-cargo-sources.py` (and check with
`--check`) after a lockfile change.

Commands where the tools and network exist:

```
flatpak install flathub org.freedesktop.Sdk//24.08 \
    org.freedesktop.Sdk.Extension.rust-stable//24.08
flatpak-builder --force-clean build-dir \
    packaging/flatpak/org.openrobotarena.OpenRobotArena.yml
flatpak-builder --user --install --force-clean build-dir \
    packaging/flatpak/org.openrobotarena.OpenRobotArena.yml
flatpak run org.openrobotarena.OpenRobotArena
flatpak run --command=ra2-shell org.openrobotarena.OpenRobotArena \
    --game "/path/to/Robot Arena 2" --route botlab --out /tmp/shots
```

### 6.2 AppImage

`packaging/appimage/build-appimage.sh [tarball]` extracts the portable tarball
under `AppDir/usr`, installs `AppRun` and the root `.desktop`/icon/metainfo,
and calls `appimagetool` (`linuxdeploy` too, if present). With linuxdeploy the
script passes `--exclude-library 'libpython3.12.so*'` and fails if a libpython
still lands in the AppDir, then reinstalls its own `AppRun` so linuxdeploy
cannot replace it.

**Decision: require the host's CPython 3.12; do not bundle it.** The binaries
carry `DT_NEEDED libpython3.12.so.1.0`, so the loader resolves it before
`main()`. Bundling would mean a second interpreter tree (libpython + stdlib),
`LD_LIBRARY_PATH`/`PYTHONHOME` wiring, choosing whose build to trust, and
tracking its security updates — the same reasons the tarball does not bundle
it. `AppRun` runs the portable launcher's `--check-python` before starting the
client and prints exactly the launcher's message on failure. Flatpak differs
because its 24.08 *runtime* already supplies 3.12; that is not a bundle the
project maintains.

Commands (two steps, on a host with `appimagetool`):

```
./tools/package.sh --no-smoke
./packaging/appimage/build-appimage.sh
chmod +x dist/ra2-open-robot-arena-0.1.0-x86_64.AppImage
./dist/ra2-open-robot-arena-0.1.0-x86_64.AppImage --game "/path/to/Robot Arena 2"
```

### 6.3 What was actually checked

This is the evidence behind the recipes, and it is not a build:

* `flatpak-builder --version` → **command not found**. `flatpak --version` is
  1.14.6; `flatpak list --runtime` has `org.freedesktop.Platform` 23.08 only
  (its `python3 --version` is 3.11.13). No `org.freedesktop.Sdk` 24.08, no
  rust-stable extension, and no network to install them.
* `command -v appimagetool` and `command -v linuxdeploy` → **nothing**;
  `build-appimage.sh` exits 2 with `NOT BUILT HERE` before staging. The
  AppImage runtime/FUSE is also fetched on first use.
* The manifest parses with PyYAML 6.0.1; `sh -n` passes on `run.sh`,
  `ra2-first-run.sh`, `AppRun` and `build-appimage.sh`.
  `desktop-file-validate` passes on the `.desktop` file. `appstreamcli
  validate --no-net` reports one warning (no homepage URL — the tree records
  no canonical project URL) and no errors.
* `gen-cargo-sources.py --check` matches `engine/Cargo.lock`: 320 registry
  packages, every checksum 64 hex digits, no duplicate destinations, no
  missing/extra crate, and the generated `cargo/config` points at
  `cargo/vendor`.
* The build's file list was checked against the tarball produced by
  `tools/package.sh --no-smoke` (22 MiB compressed, 62 MiB unpacked):
  `bin/{ra2-app,ra2-shell,ra2,ra2-render,ra2-sim,ra2-duel}`, `run.sh`, the
  `.desktop`, the 256 PNG and the SVG are all present, and there is no
  `RA2.cfz`, `.gmf`, `.bot`, `.gib` or `UI/`. The packaged `run.sh` is
  byte-identical to `packaging/run.sh`, and its `--check-python` prints
  `3.12.3`, `libpython3.12.so.1.0: found`, `CPython 3.12 stdlib: found`.
* The first-run wrapper was exercised against a fake root with a stub
  `run.sh`: explicit `--game`, the remembered path, `--check-python`
  passthrough and the no-game/no-tty error (exit 2, with the override and
  `RA2_GAME_DIR` commands). `AppRun` was exercised against a fake AppDir with
  stub `usr/run.sh` for the success and Python-failure paths. These are host
  shell tests, not Flatpak/AppImage runs.

Neither recipe is evidence of anything until it is built and the tarball's
smoke assertions can be repeated against the artifact (boot frame equality,
missing-game error, `--check-python`). Until then the tarball remains the
release.

## 7. Release checklist

```
rm -rf dist
tools/check.sh
```

`tools/check.sh` runs `cargo fmt --all -- --check`, `cargo clippy --workspace
--offline --all-targets -- -D warnings`, `cargo test --release --offline
--workspace`, `cargo build --release --offline`, then stages and smoke-tests
the tarball. `tools/package.sh --check` remains a shorter packaging-only variant
(without `-D warnings` and without the separate build step). The gate requires
the working tree clean after commit.

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

## 8. macOS and modern Windows (P3)

**Not built, not tested, and this document does not claim otherwise.** The
toolchain, the Jolt C++ build, the Python 3.12 library, the wgpu backend, game
discovery, the package shapes (`.app` + notarisation; portable zip + a Windows
launcher, or an MSI) and the exact list of unverified items are in
[`09-platforms.md`](09-platforms.md). M12 added two reviewable scripts —
`packaging/macos/bundle.sh` and `packaging/windows/zip.sh` — which each refuse
the wrong host and are marked **written, never run**; no `.app`, `.exe` or
`.zip` was produced. The only platform-motivated *Rust* change remains the
`--out` default moving from the literal `/tmp` to `std::env::temp_dir()`; no
unverified platform Rust code was invented.

## 9. One command: `tools/check.sh` (P4)

```
tools/check.sh
```

It is POSIX `sh`, uses no network, stops at the first failure, and prints a
one-line summary at the end. In order:

| Stage | What it runs |
|---|---|
| fmt | `cargo fmt --all -- --check` |
| clippy | `cargo clippy --workspace --offline --all-targets -- -D warnings` |
| tests | `cargo test --release --offline --workspace` |
| build | `cargo build --release --offline` |
| package | `tools/package.sh --no-smoke` (stage `dist/…tar.gz` + `.sha256`) |
| package smoke | `tools/smoke_package.sh` — the unpacked artifact |

The smoke test is where the headless picture checks live, reused rather than
duplicated:

* `bin/ra2-app --game local/game --offscreen PNG` boots with no display and
  writes the main-menu frame; it must be byte-identical to an independent
  `cargo run -p ra2-render --example uishot` render of
  `UI/mainmenu/mainmenu.gib`;
* `bin/ra2-shell --route botlab --out DIR --game local/game` keeps the CLI
  honest: **7** named 1280×1005 frames, `booted mainmenu/mainmenu.gib`, and
  `finished on botlab/botlabwin.gib`;
* the launcher runs the client **headlessly** through `RA2_GAME_DIR`, and its
  frame must match that same reference; without a display and without
  `--offscreen` it must refuse and name the flag;
* the missing-game errors from both `ra2-app` and the launcher must name
  `RA2.cfz`, `UI` and `Scripts`;
* `run.sh --check-python` must find `libpython3.12.so.1.0`, the 3.12 stdlib
  and a `python3.12 X.Y.Z` executable.

The final line is:

```
check: PASS — 267 tests passed, 0 failed; 7 botlab frames written; ra2-app boot frame == independent uishot render of mainmenu.gib; launcher ok; package smoke ok
```

On failure the trap prints `check: FAIL at: <stage>` and keeps the stage's
log in the terminal (the temporary directory is removed). The test count is
summed from Cargo's own `test result: ok. N passed` lines, not hard-coded.

One behavioural assertion needed a real fix rather than a rerun:
`the_shipped_ai_closes_to_contact` ran for 20 s, and on the vendored-CMake Jolt
build the first hit lands at 20.23 s (the prebuilt archive lands it just under
20 s). The window is 30 s now, so the test measures the AI closing and hitting
instead of a 0.23 s rounding margin. It is still the same assertion.

**It requires the game** (`local/game`, or `RA2_GAME_DIR`). The corpus tests
skip without it and the smoke cannot run; the script says so and exits 2
rather than reporting a smaller, misleading test count. It exports
`CARGO_NET_OFFLINE=true` so build scripts do not fetch either. This is also
why CI does **not** call `check.sh`: a checkout has no game data, so the
workflow runs the same fmt/clippy/test/build/package steps individually and
skips the smoke (`.github/workflows/ci.yml`, with that reason in a comment).

What it does **not** check: macOS or Windows (no host); Flatpak/AppImage; a
window on a real display (the client is exercised only through `--offscreen`;
the windowed loop, its input and the winit backend were not exercised here);
a clean machine with no Rust toolchain (not modelled — the smoke uses the
built artifact but the host still has the toolchain and game); other distros'
Python packaging; signing; the 3D physics results as a numeric regression (the
runners write frames and logs, which are inspected, not asserted by this
script); audio output (no device is required or tested); and visual quality
beyond the boot-frame equality. Those are named here so a green `check.sh` is
not read as more than it is.

## 10. What this round does not cover

* **Flatpak and AppImage** — full recipes, statically checked; still unbuilt
  and unverified (section 6).
* **macOS and Windows** — documented path only (section 8, docs/09); no
  build, no artifact, no test run.
* **A window on a real display.** `ra2-app` has a winit/wgpu window, but this
  smoke exercises only `--offscreen`: no window was opened, no click was
  delivered through a surface, and no display server was involved. The
  in-match C++ is not reimplemented; the shell stops at the arena picker, and
  `--match`/`--build`/`--hq` are this engine's flows.
* **Audio output.** `ra2-audio` decodes and is linked into the client, but no
  device is required or exercised (the crate's device-free fallback is the
  tested behaviour); music/video in the client is not covered.
* **Cross-distro testing.** Verified on one host (Mint 22.3 / Ubuntu 24.04,
  CPython 3.12.3). Other distros need matching 3.12 runtimes; the launcher
  says so rather than degrading.
* **arm64, musl, static linking.** Not built.
* **Bundled Python.** Deliberately not done (section 3).
* **Signing, notarisation, update feed.** None.
* **CI green on GitHub.** The workflow is written (`.github/workflows/ci.yml`)
  but has not run from this tree; macOS and Windows jobs are advisory
  (`continue-on-error`), not evidence of a working platform.
* **Game data.** Never included; the player's own copy is required.
