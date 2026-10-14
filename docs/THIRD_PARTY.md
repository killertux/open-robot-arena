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
