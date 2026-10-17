# Packaging that is documented but NOT built or tested here

Everything in this directory beyond `run.sh`, `install.sh`, the `.desktop` file,
the icon and `README.txt.in` is a **recipe, not an artifact**. The acceptance
criterion for P2 is the portable tarball built by `tools/package.sh`; Flatpak
and AppImage were not built on this machine.

## Why not

The build environment for this round has **no network access**:

* Flatpak needs `flatpak-builder`, the `org.freedesktop.Sdk` 24.08 runtime
  (~1 GB), and the Rust SDK extension. `flatpak install` downloads them.
* Cargo is run with `--offline` throughout. A Flatpak sandbox starts with an
  empty cargo cache, so the manifest would need `cargo vendor` (a network
  fetch) or a hand-populated cache. Neither was available.
* AppImage needs `appimagetool`/`linuxdeploy` and downloads the AppImage
  runtime and FUSE support on first use. Neither the tools nor the runtime are
  installed here, and there is no network to fetch them.
* Neither toolchain is installed on this machine, so even a syntactically
  valid manifest could not have been executed, and this project's rule is that
  an unmeasured claim is not a result.

The recipes are kept so the next round can verify them where the tools and the
network exist. Their presence is not evidence that they work.

## What is verified instead

`tools/package.sh` builds `dist/ra2-open-robot-arena-<version>-linux-<arch>.tar.gz`
and `tools/smoke_package.sh` unpacks it into a temp directory, runs the shell
headlessly against `local/game`, compares the boot frame with an independent
render of `mainmenu.gib`, exercises the missing-game error and the CPython 3.12
check, and asserts the layout. See `docs/08-packaging.md` for the evidence.
