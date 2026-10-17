#!/bin/sh
# Portable Windows zip layout for Open Robot Arena.
#
#   packaging/windows/zip.sh [--target-dir DIR] [--out DIR]
#
# STATUS: WRITTEN, NEVER RUN. No Windows build, zip or launch exists for this
# project. This script must run under Git Bash/MSYS/Cygwin on Windows against a
# native MSVC build; it is not a cross-packager. Its output is untested until
# the checks in docs/09-platforms.md §3 pass on Windows.
#
# It lays out the binaries and licences. It does NOT bundle CPython: the same
# decision as Linux (docs/08 §3) — the player supplies the 3.12 runtime. The
# python3.12.dll question is called out in the README it writes.
#
# No game files are copied or looked for.

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
cd "$ROOT"

TARGET=$ROOT/engine/target/release
OUT=$ROOT/dist

while [ $# -gt 0 ]; do
    case $1 in
        --target-dir)
            shift
            [ $# -gt 0 ] || { echo "zip.sh: --target-dir needs a directory" >&2; exit 2; }
            TARGET=$1
            ;;
        --out)
            shift
            [ $# -gt 0 ] || { echo "zip.sh: --out needs a directory" >&2; exit 2; }
            OUT=$1
            ;;
        -h | --help)
            sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "zip.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

case $(uname -s) in
    MINGW* | MSYS* | CYGWIN*) ;;
    *)
        echo "zip.sh: run this under Git Bash/MSYS/Cygwin on Windows (found $(uname -s))." >&2
        echo "  It expects .exe files and is not a cross-packager." >&2
        echo "  See docs/09-platforms.md §2.3 for the MSVC build prerequisites." >&2
        exit 1
        ;;
esac

VERSION=$(awk -F'"' '/^version = / { print $2; exit }' engine/Cargo.toml)
[ -n "$VERSION" ] || { echo "zip.sh: no version in engine/Cargo.toml" >&2; exit 1; }

ARCH=$(uname -m)
NAME=ra2-open-robot-arena-$VERSION-windows-$ARCH
STAGE=$OUT/$NAME
ZIP=$OUT/$NAME.zip

for bin in ra2-app ra2-shell ra2 ra2-render ra2-sim ra2-duel; do
    [ -f "$TARGET/$bin.exe" ] || {
        echo "zip.sh: $TARGET/$bin.exe is missing; build it first (MSVC):" >&2
        echo "  cd engine && cargo build --release --offline -p ra2-app -p ra2-shell -p ra2-cli -p ra2-render -p ra2sim" >&2
        exit 1
    }
done

echo "== stage $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE"

for bin in ra2-app ra2-shell ra2 ra2-render ra2-sim ra2-duel; do
    cp "$TARGET/$bin.exe" "$STAGE/$bin.exe"
done

cp LICENSE-MIT LICENSE-APACHE NOTICE "$STAGE/"
cp docs/THIRD_PARTY.md "$STAGE/THIRD_PARTY.md"

cat >"$STAGE/README.txt" <<EOF
Open Robot Arena $VERSION — Windows x86_64 (UNVERIFIED LAYOUT)

This zip was produced by packaging/windows/zip.sh. It has NOT been built,
launched or tested on Windows in this repository. Do not read its presence as
a Windows port that works.

Supply your own Robot Arena 2 installation. The game is not included. Run from
a terminal (cmd.exe or PowerShell) with the directory that directly contains
RA2.cfz, UI\ and Scripts\:

    set RA2_GAME_DIR=C:\Path\To\Robot Arena 2
    ra2-app.exe

    REM or:
    ra2-app.exe --game "C:\Path\To\Robot Arena 2"

Headless (no window; useful over SSH and for support):

    ra2-app.exe --game "C:\..." --offscreen shot.png

CPython 3.12
------------
ra2-app links CPython 3.12 through pyo3. Inspect the import:

    dumpbin /dependents ra2-app.exe | findstr /i python

Expected: python312.dll (x86_64, from python.org; not the Microsoft Store
alias). This zip does not bundle Python. Either install CPython 3.12 on the
machine, or copy python312.dll and the 3.12 Lib\ tree beside the exes and set
PYTHONHOME — that relocation was NOT tested, so the supported path is a normal
3.12 install. Do not mix a 32-bit Python with this x86_64 build.

Jolt and DX12
-------------
oxijolt-sys either fetches the prebuilt x86_64-pc-windows-msvc archive or
builds the vendored Jolt with CMake; whether the prebuilt is available and
loads was NOT measured here. The engine links Jolt statically, so no Jolt DLL
should be needed — verify against the actual build. wgpu needs a DX12 adapter;
a software/WARP adapter was not tested. The original D3D8.dll/dgVoodoo files
are never loaded or shipped.

Open work
---------
There is no run.cmd/run.ps1 in this layout: the game search and Python check
that the Linux launcher performs are still to be written and tested for
Windows. See docs/09-platforms.md §2.3, §3.
EOF

echo "== zip $ZIP"
rm -f "$ZIP"
if command -v zip >/dev/null 2>&1; then
    (cd "$OUT" && zip -q -r "$NAME.zip" "$NAME")
elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -Command \
        "Compress-Archive -Path '$STAGE' -DestinationPath '$ZIP' -Force"
else
    echo "zip.sh: neither zip nor powershell.exe is available" >&2
    exit 1
fi

echo "NOT VERIFIED: no Windows build, package or run exists for this zip."
echo
ls -l "$ZIP"
