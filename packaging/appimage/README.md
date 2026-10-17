# AppImage recipe — written and checked, **NOT BUILT HERE**

`build-appimage.sh` turns the portable tarball into
`dist/ra2-open-robot-arena-<version>-x86_64.AppImage`. The AppDir is the
tarball under `usr/`, with `AppRun`, a `.desktop` file, the PNG/SVG icon and the
AppStream metainfo at the root.

## What is missing on this machine

```
$ command -v appimagetool
(nothing)
$ command -v linuxdeploy
(nothing)
```

`appimagetool` is required; `linuxdeploy` is optional (used for dependency
deployment when present). The AppImage runtime/FUSE is fetched on first use.
The script therefore exits 2 with `NOT BUILT HERE` before staging anything.

## Python 3.12 decision

**The AppImage requires the host's CPython 3.12 and does not bundle it.**

* `ra2-app` (and `ra2-shell`, `ra2-sim`, `ra2-duel`) carry
  `DT_NEEDED libpython3.12.so.1.0`. The loader resolves it before `main()`;
  `--no-python` does not remove it.
* Bundling would mean a second interpreter tree (libpython + stdlib),
  `LD_LIBRARY_PATH`/`PYTHONHOME` wiring, choosing whose build to trust and
  tracking its security updates. The measured project rule is the system
  runtime (`docs/08-packaging.md` §3); Flatpak's 24.08 runtime supplies 3.12,
  and the tarball launcher requires it.
* `AppRun` runs the portable launcher's `--check-python` before starting the
  client. On failure it prints exactly the launcher's message (missing
  `libpython3.12.so.1.0` and/or the 3.12 stdlib, plus the distro install
  commands). `run.sh --check-python` prints the same guidance when invoked
  directly.

Bundling CPython remains a separate decision; if taken, `AppRun` and this
script must change together (set `PYTHONHOME`/`LD_LIBRARY_PATH` to the bundled
tree) and the result must be measured.

## Build

On a host with `appimagetool`:

```sh
./tools/package.sh --no-smoke
./packaging/appimage/build-appimage.sh
```

or pass a tarball explicitly:

```sh
./packaging/appimage/build-appimage.sh dist/ra2-open-robot-arena-0.1.0-linux-x86_64.tar.gz
```

With `linuxdeploy` installed it is run first, with
`--exclude-library 'libpython3.12.so*'`; the script fails if a libpython ends up
in the AppDir. `AppRun` is then reinstalled so linuxdeploy cannot replace it.

Run the result:

```sh
chmod +x dist/ra2-open-robot-arena-0.1.0-x86_64.AppImage
./dist/ra2-open-robot-arena-0.1.0-x86_64.AppImage --game "/path/to/Robot Arena 2"
```

The game is chosen at run time with `--game` (or `RA2_GAME_DIR`), exactly as in
the tarball. It is never bundled and AppRun has no portal wrapper: an AppImage
is not sandboxed, so the normal filesystem is already visible.

## What would count as verified

An AppImage built by this script on a machine with the tools, then run on a
clean host (no Rust toolchain, no game inside) through `--offscreen` and the
missing-game case. No AppImage exists today; the tarball remains the release.
