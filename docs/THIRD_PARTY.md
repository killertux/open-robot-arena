# Third-party components

This project builds on community reverse-engineering work. We do not claim it.

## PyRA2 — Robot Arena 2 file-format libraries
- Author: Martijn Pieters (`pyra2@zopatista.com`)
- Source: https://sourceforge.net/projects/pyra2/ (release 0.1.1, 2003)
- License: **GNU LGPL v2.1 or later** (see `tools/vendor/PyRA2/PyRA2-0.1.1/LICENSE.txt`)
- Vendored under `tools/vendor/PyRA2/` **as reference documentation/spec** for the
  GMI/GMF mesh format and the `.bot` robot format. Our own `tools/gmi.py` is an
  independent Python 3 reimplementation derived from the same format analysis.
- Because it is LGPL, PyRA2 code is kept isolated in `tools/vendor/`; if we reuse
  it as a linked library we will honour the LGPL terms (source + notice).

## Robot Arena 2 (the game)
- © Gabriel Interactive / Infogrames. **Not** distributed by this project.
- Game files live only in `local/` (git-ignored) and are supplied by the user.

## Bundled mod tools (shipped with the game repack, not ours)
- Dummy's GMF Compiler, Dummy's .BOTedit, Lu-Tze CFZ Compiler,
  MakeCFL/UnCFL (Jari Komppa), Component Exporter — present in `local/game/#Tools/`,
  used only as behavioural references.

## Jolt Physics
- Author: Jorrit Rouwe. Source: https://github.com/jrouwe/JoltPhysics
- License: **MIT**.
- Used as the Havok substitute for the engine's rigid-body simulation. We link
  it through the safe Rust bindings **`oxijolt`** (MIT OR Apache-2.0), which
  wraps the `joltc` C API. Neither Jolt's source nor its binaries are vendored
  into this repository; they are pulled in as ordinary Cargo dependencies.

## CPython, and `pyo3`
- CPython: © the Python Software Foundation. License: **PSF License v2**
  (GPL-compatible, permissive).
- `pyo3`: https://github.com/PyO3/pyo3 — License: **MIT OR Apache-2.0**.
- `ra2script` embeds a CPython 3 interpreter to run the game's own Python
  scripts and exposes the engine to them as the `plus` module. The interpreter is
  the *user's* system CPython; neither it nor its standard library is vendored or
  redistributed here. The game's shipped Python code (`Arenas/`, `AI/`,
  `Scripts/`) is **the game's**, lives in `local/` (git-ignored), and is executed
  as data by the player's own installation — we do not redistribute it either.
- `build.rs` creates a `libpythonX.Y.so` symlink inside the build directory when a
  distribution ships only the versioned soname (Ubuntu's `libpython3.12t64`
  without `python3.12-dev`); nothing is written outside `target/`.

## CFL3 archive format (`.cfz`)
- Author: Jari Komppa ("Compressed File Library 3").
- Source: https://github.com/jarikomppa/cfl — License: **zlib** (permissive).
- We do **not** bundle or link his C++ code; `engine/crates/ra2fmt/src/cfl.rs` is
  an independent Rust reimplementation written from the documented on-disk format.
- Acknowledgment: the `.cfz` container format and its zlib filter are his design.
