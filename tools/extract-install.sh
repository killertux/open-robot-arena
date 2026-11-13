#!/bin/sh
# Unpack a Robot Arena 2 installer into a game folder, without running it.
#
#   tools/extract-install.sh [--force] <installer> <game_dir>
#
# <installer>: the "LGU Repack" download (ra2_repack.zip, the folder it unzips
# to, or its "Game Files/Robot Arena 2.exe"), or an .msi with its .cab beside
# it. Then play with:
#
#   engine/target/release/ra2-app --game <game_dir>
#
# Needs only python3 (any 3.x; the port itself needs 3.12 anyway): no Wine,
# msitools or cabextract, so it works on macOS, Linux and Windows (Git Bash).
# The installer is only read. See tools/extract_install.py for the format.

set -eu

PY=${PYTHON:-python3}
command -v "$PY" >/dev/null 2>&1 || { echo "extract-install.sh: python3 not found (set PYTHON=...)" >&2; exit 2; }
exec "$PY" -I "$(dirname -- "$0")/extract_install.py" "$@"
