# Packaging that is documented but NOT built or tested here

Everything in this directory beyond `run.sh`, `install.sh`, the `.desktop` file,
the icon and `README.txt.in` is a **recipe, not an artifact**. The acceptance
criterion for P2 is the portable tarball built by `tools/package.sh`; Flatpak
and AppImage were not built on this machine.

M12.3 completed both recipes and checked them statically. They are still not
artifacts. The details and the static evidence are in
[`docs/08-packaging.md`](../docs/08-packaging.md) §6.

## Flatpak (`flatpak/`)

`org.openrobotarena.OpenRobotArena.yml` is a complete manifest:

* runtime `org.freedesktop.Platform`/`org.freedesktop.Sdk` **24.08**, which
  carries CPython 3.12 (the installed 23.08 has 3.11.13, so it is the wrong
  runtime);
* `cargo build --release --offline` using `cargo-sources.json`, generated from
  `engine/Cargo.lock` by `gen-cargo-sources.py` (the sandbox starts with an
  empty `CARGO_HOME`; the JSON supplies 320 crates.io archives and a vendored
  source config);
* installs `ra2-app` + the five tools, `run.sh`, a first-run wrapper that
  chooses the game through the desktop portal and remembers it, the `.desktop`
  entry, PNG/SVG icons and AppStream metainfo;
* `finish-args` for Wayland/X11, PulseAudio, DRI, `home:ro` and the portal
  (no network).

**Not built here:** `flatpak-builder` is not installed (`flatpak-builder
--version` → command not found), the 24.08 SDK and Rust extension are not
installed, and there is no network to fetch them. No `.flatpak`, build
directory or installed runtime exists.

## AppImage (`appimage/`)

`build-appimage.sh` extracts the tarball into an AppDir, adds `AppRun`,
`.desktop`, icon and metainfo, optionally runs `linuxdeploy` (with CPython
excluded), and calls `appimagetool`. `AppRun` runs the portable launcher's
`--check-python` and prints the launcher's own message on failure.

**Decision:** the AppImage requires the host's CPython 3.12 and does **not**
bundle an interpreter. The binaries carry `DT_NEEDED libpython3.12.so.1.0`, so
the loader resolves it before `main()`. Bundling would add a second
interpreter tree, `PYTHONHOME`/`LD_LIBRARY_PATH` wiring and a security-update
burden; the measured project rule is the system runtime. (Flatpak is different:
its 24.08 runtime already supplies 3.12.)

**Not built here:** neither `appimagetool` nor `linuxdeploy` is installed
(checked with `command -v`), and the AppImage runtime/FUSE is fetched on first
use. The script exits 2 with `NOT BUILT HERE` before staging.

## What is verified instead

`tools/package.sh` builds `dist/ra2-open-robot-arena-<version>-linux-<arch>.tar.gz`
and `tools/smoke_package.sh` unpacks it into a temp directory, runs the shell
headlessly against `local/game`, compares the boot frame with an independent
render of `mainmenu.gib`, exercises the missing-game error and the CPython 3.12
check, and asserts the layout. The Flatpak/AppImage build file lists were
checked against that tarball; the recipes were not executed. See
`docs/08-packaging.md` for the evidence.
