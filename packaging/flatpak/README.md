# Flatpak recipe — written and statically checked, **NOT BUILT HERE**

The manifest is
`org.openrobotarena.OpenRobotArena.yml`. The app id is
`org.openrobotarena.OpenRobotArena`; the command is `ra2-open-robot-arena`,
the first-run wrapper that hands the game directory to `run.sh` as `--game`.

## What is missing on this machine

```
$ flatpak --version
Flatpak 1.14.6
$ flatpak-builder --version
flatpak-builder: command not found
$ flatpak list --runtime
org.freedesktop.Platform 23.08   (Python 3.11.13; verified in the runtime)
```

* `flatpak-builder` — **missing** (the only hard blocker to even a dry run).
* `org.freedesktop.Sdk//24.08` and
  `org.freedesktop.Sdk.Extension.rust-stable//24.08` — **not installed**;
  installing them needs network (~1 GB).
* No network in this build round. The recipe's sources are the repository and
  `cargo-sources.json`; a real build also fetches the runtime and the crate
  archives.

So no `.flatpak`, no build directory, and no installed runtime were produced
here. The YAML and the shell scripts were parsed, and the file list was checked
against the tarball (see below). That is not a build.

## Python 3.12 question

The binaries link `libpython3.12.so.1.0` as `DT_NEEDED`; the dynamic loader
resolves it before `main()`, so `--no-python` cannot avoid it.

**Runtime `24.08` is the right one.** freedesktop-sdk 24.08 carries CPython
3.12 (release 24.08.21 updated python3 to 3.12.11-0). The installed 23.08
runtime was checked here and has **Python 3.11.13**, which cannot satisfy a
3.12 link — that is why the manifest is not on 23.08. The SDK half of 24.08
supplies `python3.12`, its headers and `python3.pc`, which `pyo3` uses to find
the interpreter at build time.

This is a *known fact from the runtime's release notes*, not a build result:
24.08 was not installed here. If a future 24.08 update moved to another Python
minor, the build would fail at `pyo3`'s link, or the app would demand a
different libpython; the fix is to rebuild against the runtime's Python, not to
bundle one. The Flatpak does **not** bundle CPython — it uses the runtime's.

The wrapper and `run.sh` still perform the launcher's check (`ldd` for the
soname and a 3.12 stdlib). Inside the Flatpak those checks are expected to
pass against `/usr/lib` from the runtime.

## The game directory: `home:ro` plus the portal chooser

A Flatpak sandbox cannot browse the host, and the game is never bundled. The
manifest uses:

* `--filesystem=home:ro` — the blunt grant. It covers the documented default
  `~/.local/share/ra2-open-robot-arena/game` and any install under `$HOME`,
  read-only. It is **not** `--filesystem=home` (read-write) and not a static
  per-path grant, because the player's path is unknown at build time.
* `--env=GTK_USE_PORTAL=1` and `zenity` in the runtime: on first run
  `ra2-first-run.sh` opens a directory chooser through the XDG desktop portal.
  The chosen directory is exported by the document portal, so the sandbox can
  read it even outside `$HOME`; the path is stored and passed as `--game`.
  Document-portal paths (`/run/user/.../doc/...`) can go stale across sessions;
  the wrapper validates the saved path every run and asks again if it no
  longer contains `RA2.cfz`, `UI/` and `Scripts/`.

A game outside `$HOME` is most durable with one user override:

```sh
flatpak override --user --filesystem="/path/to/Robot Arena 2:ro" \
    org.openrobotarena.OpenRobotArena
```

The choice is stored at
`$XDG_CONFIG_HOME/ra2-open-robot-arena/game-dir`, i.e. on the host under
`~/.var/app/org.openrobotarena.OpenRobotArena/config/ra2-open-robot-arena/game-dir`.
The game is read in place; nothing is copied into the sandbox or the app.

The blunt `home:ro` grant is a v1.0 trade-off, stated plainly: it lets the app
read the whole home directory, not only the game. A narrower recipe would need
the app itself to implement a portal file chooser and keep a document-portal
handle, which is not in this engine today. The chooser above is the portal
path that works without changing the client.

## Offline Cargo

A Flatpak sandbox starts with an empty `CARGO_HOME`. `cargo-sources.json` (641
entries: 320 crates.io archives, 320 inline `.cargo-checksum.json` files and a
`cargo/config`) is generated from `engine/Cargo.lock` by
`gen-cargo-sources.py`. flatpak-builder extracts each crate under
`cargo/vendor/` and writes `cargo/config`; the manifest sets
`CARGO_HOME=/run/build/ra2-open-robot-arena/cargo`, so the build command is
exactly:

```sh
cargo build --release --offline --manifest-path engine/Cargo.toml \
    -p ra2-app -p ra2-shell -p ra2-cli -p ra2-render -p ra2sim
```

`CARGO_NET_OFFLINE=true` is also set: `--offline` does not reach a Cargo build
script, and `oxijolt-sys` would otherwise fetch its optional prebuilt Jolt
archive. With it, Jolt 5.6.0 is built from the vendored CMake sources. The
manifest skips `local/`, `dist/`, `downloads/`, `engine/target` and `.git`, so
the player's game and old build output never enter the sandbox.

Regenerate after a lockfile change:

```sh
./packaging/flatpak/gen-cargo-sources.py
./packaging/flatpak/gen-cargo-sources.py --check
```

## Build and run (where the tools exist)

```sh
# once, needs network
flatpak install flathub org.freedesktop.Sdk//24.08 \
    org.freedesktop.Sdk.Extension.rust-stable//24.08

# build only
flatpak-builder --force-clean build-dir \
    packaging/flatpak/org.openrobotarena.OpenRobotArena.yml

# build and install for the user
flatpak-builder --user --install --force-clean build-dir \
    packaging/flatpak/org.openrobotarena.OpenRobotArena.yml

flatpak run org.openrobotarena.OpenRobotArena
flatpak run --env=RA2_GAME_DIR="/path/to/Robot Arena 2" \
    org.openrobotarena.OpenRobotArena
flatpak run --command=ra2-shell org.openrobotarena.OpenRobotArena \
    --game "/path/to/Robot Arena 2" --route botlab --out /tmp/shots
```

The first `flatpak run` asks for the game directory. `--game` on the command
line also works and is remembered. `flatpak run --command=ra2-shell` (and
`ra2`, `ra2-render`, `ra2-sim`, `ra2-duel`, or `ra2-app` directly) reaches the
tools; those bypass the first-run picker, so pass `--game` or set
`RA2_GAME_DIR`.

Desktop entry: `org.openrobotarena.OpenRobotArena.desktop`, installed from the
project's `.desktop` with the desktop file renamed to the app id; the icon is
installed as `ra2-open-robot-arena` (PNG 256 and SVG), matching the desktop
file's `Icon=`. AppStream metainfo is installed to `/app/share/metainfo/`.

## What was actually checked here

* `python3 -c 'import yaml; yaml.safe_load(...)'` — the manifest parses
  (PyYAML 6.0.1).
* `sh -n` on `ra2-first-run.sh`, `run.sh`, `AppRun` and
  `build-appimage.sh`.
* `appstreamcli validate --no-net` on the metainfo (no network).
* `desktop-file-validate` on `packaging/ra2-open-robot-arena.desktop`.
* `gen-cargo-sources.py --check` — the JSON matches `engine/Cargo.lock`; every
  registry package has a checksum and a crates.io URL.
* The six binaries, `run.sh`, desktop, icon, metainfo and `tools/make_icon.py`
  used by the build commands were checked against the tarball produced by
  `tools/package.sh --no-smoke` (and `engine/Cargo.toml` for the workspace
  members).
* The first-run wrapper was run against a fake root with a stub `bin/ra2-app`
  to confirm `--game`, the saved choice and the argument passthrough. This
  was a host shell test, not a Flatpak run.

## What would count as verified

`flatpak-builder` succeeds, the app installs, `flatpak run --command=ra2-app`
boots with `--offscreen` against a real game directory, the first-run chooser
returns a readable path, and the missing-game error names `RA2.cfz`, `UI` and
`Scripts`. Until then, the tarball remains the release.
