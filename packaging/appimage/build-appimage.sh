#!/bin/sh
# Build the Open Robot Arena AppImage from the portable tarball.
#
#   ./packaging/appimage/build-appimage.sh [dist/ra2-open-robot-arena-*.tar.gz]
#
# With no argument, the single dist/ra2-open-robot-arena-*-linux-x86_64.tar.gz
# is used; build it first with ./tools/package.sh --no-smoke.
#
# STATUS: WRITTEN AND STATICALLY CHECKED, NOT BUILT HERE.  This host has
# neither appimagetool nor linuxdeploy (checked with `command -v`), and the
# AppImage runtime/FUSE is downloaded on first use.  See
# packaging/appimage/README.md and docs/08-packaging.md §6.
#
# The CPython 3.12 decision: the AppImage requires the host's CPython 3.12 and
# does NOT bundle the interpreter.  AppRun runs the portable launcher's own
# `--check-python`, so a missing runtime gets exactly the launcher's message.
# Bundling a relocatable 3.12 would mean shipping libpython + stdlib, setting
# PYTHONHOME/LD_LIBRARY_PATH, choosing a build to trust and tracking its
# security updates; the repository's measured rule (docs/08 §3) is to use the
# system runtime, and Flatpak's 24.08 runtime supplies 3.12 as well.  This is
# therefore the one declared runtime requirement, checked before the client
# starts.  If a future release bundles CPython, this script and AppRun must
# change together and the tarball's rule remains the fallback.

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
APP=ra2-open-robot-arena

TARBALL=${1:-}
if [ -z "$TARBALL" ]; then
    # Refuse ambiguity rather than picking one of several versions.
    set -- "$ROOT"/dist/ra2-open-robot-arena-*-linux-x86_64.tar.gz
    if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
        echo "build-appimage.sh: pass one tarball, or run tools/package.sh --no-smoke first" >&2
        echo "  found: $*" >&2
        exit 2
    fi
    TARBALL=$1
fi
[ -f "$TARBALL" ] || {
    echo "build-appimage.sh: $TARBALL does not exist" >&2
    exit 2
}

APPIMAGETOOL=
for candidate in appimagetool appimagetool-x86_64.AppImage; do
    if command -v "$candidate" >/dev/null 2>&1; then
        APPIMAGETOOL=$candidate
        break
    fi
done
[ -n "$APPIMAGETOOL" ] || {
    echo "build-appimage.sh: appimagetool is not installed; this recipe is NOT BUILT HERE." >&2
    echo "  install appimagetool (or appimagetool-x86_64.AppImage) and rerun." >&2
    exit 2
}

# Both appimagetool and linuxdeploy are commonly distributed as AppImages.
# Let them extract themselves when FUSE is unavailable.
APPIMAGE_EXTRACT_AND_RUN=${APPIMAGE_EXTRACT_AND_RUN:-1}
export APPIMAGE_EXTRACT_AND_RUN

VERSION=$(awk -F'"' '/^version = / { print $2; exit }' "$ROOT/engine/Cargo.toml")
[ -n "$VERSION" ] || {
    echo "build-appimage.sh: no version in engine/Cargo.toml" >&2
    exit 1
}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
APPDIR=$WORK/AppDir
mkdir -p "$APPDIR/usr" "$ROOT/dist"

echo "== unpack $TARBALL"
tar -xzf "$TARBALL" -C "$APPDIR/usr" --strip-components=1
[ -x "$APPDIR/usr/bin/ra2-app" ] || {
    echo "build-appimage.sh: the tarball has no usr/bin/ra2-app" >&2
    exit 1
}
[ -x "$APPDIR/usr/run.sh" ] || {
    echo "build-appimage.sh: the tarball has no usr/run.sh" >&2
    exit 1
}

echo "== AppDir metadata"
install -m755 "$ROOT/packaging/appimage/AppRun" "$APPDIR/AppRun"
install -m644 "$ROOT/packaging/ra2-open-robot-arena.desktop" \
    "$APPDIR/$APP.desktop"
install -m644 "$ROOT/packaging/ra2-open-robot-arena.svg" "$APPDIR/$APP.svg"
if [ -f "$APPDIR/usr/share/icons/hicolor/256x256/apps/$APP.png" ]; then
    install -m644 "$APPDIR/usr/share/icons/hicolor/256x256/apps/$APP.png" \
        "$APPDIR/$APP.png"
else
    # package.sh always writes it; this keeps a hand-staged AppDir working.
    python3 "$ROOT/tools/make_icon.py" "$APPDIR/$APP.png"
fi
install -Dm644 \
    "$ROOT/packaging/flatpak/org.openrobotarena.OpenRobotArena.metainfo.xml" \
    "$APPDIR/usr/share/metainfo/org.openrobotarena.OpenRobotArena.metainfo.xml"

# linuxdeploy is optional: its useful work here is copying non-system
# libraries.  CPython is excluded on purpose — bundling it would contradict
# the declared system-3.12 requirement and can silently ship a different ABI.
# Run it before installing the authoritative AppRun/desktop/icon so its
# output cannot replace them.
LINUXDEPLOY=
for candidate in linuxdeploy linuxdeploy-x86_64.AppImage; do
    if command -v "$candidate" >/dev/null 2>&1; then
        LINUXDEPLOY=$candidate
        break
    fi
done
if [ -n "$LINUXDEPLOY" ]; then
    echo "== linuxdeploy (CPython excluded)"
    "$LINUXDEPLOY" --appdir "$APPDIR" \
        --executable "$APPDIR/usr/bin/ra2-app" \
        --executable "$APPDIR/usr/bin/ra2-shell" \
        --executable "$APPDIR/usr/bin/ra2" \
        --executable "$APPDIR/usr/bin/ra2-render" \
        --executable "$APPDIR/usr/bin/ra2-sim" \
        --executable "$APPDIR/usr/bin/ra2-duel" \
        --desktop-file "$APPDIR/$APP.desktop" \
        --icon-file "$APPDIR/$APP.png" \
        --exclude-library 'libpython3.12.so*'
    if find "$APPDIR" -name 'libpython3.12.so*' -print -quit | grep -q .; then
        echo "build-appimage.sh: linuxdeploy bundled libpython3.12 despite the exclude;" >&2
        echo "  this AppImage requires the host's CPython 3.12, not a bundled one." >&2
        exit 1
    fi
    install -m755 "$ROOT/packaging/appimage/AppRun" "$APPDIR/AppRun"
    install -m644 "$ROOT/packaging/ra2-open-robot-arena.desktop" \
        "$APPDIR/$APP.desktop"
    install -m644 "$ROOT/packaging/ra2-open-robot-arena.svg" "$APPDIR/$APP.svg"
    install -m644 "$APPDIR/usr/share/icons/hicolor/256x256/apps/$APP.png" \
        "$APPDIR/$APP.png" 2>/dev/null || true
else
    echo "== linuxdeploy not found; using the tarball's binaries as-is"
fi

# appimagetool may itself be an AppImage; APPIMAGE_EXTRACT_AND_RUN (set
# above) lets it run without FUSE.
OUT=$ROOT/dist/ra2-open-robot-arena-$VERSION-x86_64.AppImage
rm -f "$OUT" "$OUT.sha256"
echo "== $APPIMAGETOOL $APPDIR"
ARCH=x86_64 "$APPIMAGETOOL" --no-appstream "$APPDIR" "$OUT"
(cd "$ROOT/dist" && sha256sum "$(basename "$OUT")" >"$(basename "$OUT").sha256")

echo
echo "appimage: $OUT"
cat "$OUT.sha256"
echo
echo "Run it:"
echo "  chmod +x \"$OUT\""
echo "  \"$OUT\" --game \"/path/to/Robot Arena 2\""
echo "It needs the host's CPython 3.12 (libpython3.12.so.1.0 and the stdlib);"
echo "AppRun runs the launcher's check and prints its message if it is missing."
