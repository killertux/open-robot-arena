#!/bin/sh
# NOT BUILT HERE — documented AppImage recipe.
#
# This script was not run: appimagetool/linuxdeploy are not installed on this
# machine, the AppImage runtime and FUSE are downloaded on first use, and this
# round has no network. See packaging/NOT_BUILT.md. It is kept as the recipe a
# later round can verify.
#
#   ./packaging/appimage/build-appimage.sh dist/ra2-open-robot-arena-*.tar.gz
#
# The AppImage would still dynamically link the host's libpython3.12.so.1.0
# unless the recipe is extended to bundle CPython 3.12 (and its stdlib) inside
# the AppDir. That bundling is a licensing/size decision, not a flag, and was
# not measured here; the tarball's documented "CPython 3.12 is required" rule
# is the tested one.

set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
TARBALL=${1:-}
[ -n "$TARBALL" ] && [ -f "$TARBALL" ] || {
    echo "usage: build-appimage.sh dist/ra2-open-robot-arena-*.tar.gz" >&2
    exit 2
}
command -v appimagetool >/dev/null 2>&1 || {
    echo "build-appimage.sh: appimagetool is not installed (this recipe is NOT built here)" >&2
    exit 2
}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/AppDir"
tar -xzf "$TARBALL" -C "$WORK/AppDir" --strip-components=1
ln -sf run.sh "$WORK/AppDir/AppRun"
cp "$WORK/AppDir/share/applications/ra2-open-robot-arena.desktop" "$WORK/AppDir/"
cp "$WORK/AppDir/share/icons/hicolor/256x256/apps/ra2-open-robot-arena.png" "$WORK/AppDir/"

ARCH=x86_64 appimagetool "$WORK/AppDir" "$ROOT/dist/ra2-open-robot-arena-x86_64.AppImage"
