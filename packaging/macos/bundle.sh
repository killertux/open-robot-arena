#!/bin/sh
# macOS .app skeleton for Open Robot Arena.
#
#   packaging/macos/bundle.sh [--target-dir DIR] [--out DIR]
#
# STATUS: WRITTEN, NEVER RUN. This script has not been executed on macOS and
# no .app produced by it has been signed, notarised or started. It exists so
# the intended layout is reviewable and so a macOS build can be attempted
# without inventing a second set of paths. Treat its output as untested until
# the checks in docs/09-platforms.md §3 have actually passed on a Mac.
#
# What it does NOT do:
#   * it does not build; run `cargo build --release -p ra2-app ...` first;
#   * it does not copy or repair libpython3.12.dylib (see the README it writes);
#   * it does not create an .icns, sign, or notarise.
#
# The game is never copied. The app reads it through --game or RA2_GAME_DIR
# (there is no game/ inside the bundle).

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
cd "$ROOT"

TARGET=$ROOT/engine/target/release
OUT=$ROOT/dist

while [ $# -gt 0 ]; do
    case $1 in
        --target-dir)
            shift
            [ $# -gt 0 ] || { echo "bundle.sh: --target-dir needs a directory" >&2; exit 2; }
            TARGET=$1
            ;;
        --out)
            shift
            [ $# -gt 0 ] || { echo "bundle.sh: --out needs a directory" >&2; exit 2; }
            OUT=$1
            ;;
        -h | --help)
            sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "bundle.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

if [ "$(uname -s)" != "Darwin" ]; then
    echo "bundle.sh: this script must run on macOS (found $(uname -s))." >&2
    echo "  It is not a cross-packager, and no macOS artifact was built here." >&2
    echo "  See docs/09-platforms.md §2.2 for the build prerequisites." >&2
    exit 1
fi

VERSION=$(awk -F'"' '/^version = / { print $2; exit }' engine/Cargo.toml)
[ -n "$VERSION" ] || { echo "bundle.sh: no version in engine/Cargo.toml" >&2; exit 1; }

APP_NAME="Open Robot Arena"
BUNDLE=$OUT/"$APP_NAME.app"
MACOS=$BUNDLE/Contents/MacOS
RES=$BUNDLE/Contents/Resources

for bin in ra2-app ra2-shell ra2 ra2-render ra2-sim ra2-duel; do
    [ -x "$TARGET/$bin" ] || {
        echo "bundle.sh: $TARGET/$bin is missing; build it first:" >&2
        echo "  cd engine && cargo build --release --offline -p ra2-app -p ra2-shell -p ra2-cli -p ra2-render -p ra2sim" >&2
        exit 1
    }
done

echo "== stage $BUNDLE"
rm -rf "$BUNDLE"
mkdir -p "$MACOS/bin" "$RES"

cp "$TARGET/ra2-app" "$MACOS/ra2-app"
chmod 755 "$MACOS/ra2-app"
for bin in ra2-shell ra2 ra2-render ra2-sim ra2-duel; do
    cp "$TARGET/$bin" "$MACOS/bin/$bin"
    chmod 755 "$MACOS/bin/$bin"
done

cat >"$BUNDLE/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>org.openrobotarena.OpenRobotArena</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>ra2-app</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

cp packaging/ra2-open-robot-arena.svg "$RES/ra2-open-robot-arena.svg"
cp LICENSE-MIT LICENSE-APACHE NOTICE "$RES/"
cp docs/THIRD_PARTY.md "$RES/THIRD_PARTY.md"

cat >"$RES/README-MACOS.txt" <<EOF
Open Robot Arena $VERSION — macOS bundle (UNVERIFIED SKELETON)

This .app was produced by packaging/macos/bundle.sh. It has NOT been run,
signed, notarised or tested on macOS in this repository.

Running it needs a Robot Arena 2 install (RA2.cfz, UI/, Scripts/) supplied by
you. From a terminal:

    RA2_GAME_DIR="/path/to/Robot Arena 2" "$BUNDLE/Contents/MacOS/ra2-app"

Finder launch has no environment: the binary then searches local/game upwards
from the working directory and \$RA2_GAME_DIR, which usually means it reports a
missing game. A Finder launcher that asks for the directory is still open work.

Python 3.12
-----------
ra2-app links CPython 3.12 through pyo3. Check:

    otool -L "$BUNDLE/Contents/MacOS/ra2-app" | grep libpython

Expected: libpython3.12.dylib. A python.org framework Python records an
absolute install name; a Homebrew/relocatable Python may need the dylib copied
to Contents/Frameworks and either an install_name_tool change or
DYLD_LIBRARY_PATH + PYTHONHOME. NONE OF THIS WAS MEASURED HERE.

Jolt and Metal
--------------
oxijolt-sys has no macOS prebuilt archive in prebuilt.txt, so the build uses
the vendored Jolt 5.6.0 through CMake (a static library; no Jolt dylib should be
needed — verify with otool). wgpu must find a Metal adapter, including in a
headless/SSH session. Both are unverified.

Signing and notarisation
------------------------
Distribution outside the build machine needs:

    codesign --force --deep --options runtime --sign "Developer ID Application: ..." "$APP_NAME.app"
    xcrun notarytool submit "$APP_NAME.zip" --wait --keychain-profile ...
    xcrun stapler staple "$APP_NAME.app"

Not run here. See docs/09-platforms.md §2.2 and §3 for what would close these
gaps.
EOF

echo "== $BUNDLE"
echo "NOT VERIFIED: no macOS build, code signature or run exists for this bundle."
echo
find "$BUNDLE" -type f | sed "s|$BUNDLE/||" | sort
