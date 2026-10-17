#!/bin/sh
# Build the portable Linux tarball under dist/.
#
#   ./tools/package.sh             build, package, run the smoke test
#   ./tools/package.sh --check     also cargo fmt --check, clippy, cargo test
#   ./tools/package.sh --no-smoke  only build and package (for a cross build)
#
# The result is dist/ra2-open-robot-arena-<version>-linux-<arch>.tar.gz plus a
# .sha256. Nothing is downloaded: cargo runs --offline. The game is never read
# or copied; the smoke test reads local/game in place.

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$ROOT"

ARCH=$(uname -m)
VERSION=$(awk -F'"' '/^version = / { print $2; exit }' engine/Cargo.toml)
[ -n "$VERSION" ] || { echo "package.sh: no version in engine/Cargo.toml" >&2; exit 1; }

NAME=ra2-open-robot-arena-$VERSION-linux-$ARCH
DIST=$ROOT/dist
STAGE=$DIST/$NAME
TARBALL=$DIST/$NAME.tar.gz

EPOCH=${SOURCE_DATE_EPOCH:-$(date +%s)}
BUILD_DATE=$(date -u -d "@$EPOCH" +%Y-%m-%d)

mode=package
smoke=1
for arg in "$@"; do
    case $arg in
        --check) mode=check ;;
        --no-smoke) smoke=0 ;;
        -h | --help)
            sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "package.sh: unknown argument $arg" >&2
            exit 2
            ;;
    esac
done

if [ "$(uname -s)" != "Linux" ]; then
    echo "package.sh: this package is Linux-only (P2); found $(uname -s)." >&2
    echo "  macOS and Windows are P3 and are not built here." >&2
    exit 1
fi

if [ "$mode" = check ]; then
    echo "== fmt"
    (cd engine && cargo fmt --all -- --check)
    echo "== clippy"
    (cd engine && cargo clippy --workspace --offline --all-targets)
    echo "== tests (release)"
    (cd engine && cargo test --release --offline --workspace)
fi

echo "== build $NAME"
cargo build --release --offline --manifest-path engine/Cargo.toml \
    -p ra2-shell -p ra2-cli -p ra2-render -p ra2sim

for bin in ra2-shell ra2 ra2-render ra2-sim ra2-duel; do
    [ -x "engine/target/release/$bin" ] || {
        echo "package.sh: engine/target/release/$bin is missing" >&2
        exit 1
    }
done

echo "== stage $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE/bin" \
    "$STAGE/share/applications" \
    "$STAGE/share/icons/hicolor/256x256/apps" \
    "$STAGE/share/icons/hicolor/scalable/apps"

for bin in ra2-shell ra2 ra2-render ra2-sim ra2-duel; do
    cp "engine/target/release/$bin" "$STAGE/bin/$bin"
    chmod 755 "$STAGE/bin/$bin"
done

cp packaging/run.sh "$STAGE/run.sh"
cp packaging/install.sh "$STAGE/install.sh"
chmod 755 "$STAGE/run.sh" "$STAGE/install.sh"
cp packaging/ra2-open-robot-arena.desktop "$STAGE/share/applications/"
cp packaging/ra2-open-robot-arena.svg \
    "$STAGE/share/icons/hicolor/scalable/apps/ra2-open-robot-arena.svg"
cp LICENSE-MIT LICENSE-APACHE NOTICE "$STAGE/"
cp docs/THIRD_PARTY.md "$STAGE/THIRD_PARTY.md"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$BUILD_DATE/g" \
    packaging/README.txt.in > "$STAGE/README.txt"

echo "== icon"
python3 tools/make_icon.py \
    "$STAGE/share/icons/hicolor/256x256/apps/ra2-open-robot-arena.png"

echo "== tarball $TARBALL"
rm -f "$TARBALL" "$TARBALL.sha256"
# Sorted members and a fixed mtime/owner make the archive reproducible when
# SOURCE_DATE_EPOCH is set (GNU tar; this is a Linux package).
tar --sort=name --format=gnu --owner=0 --group=0 --numeric-owner \
    --mtime="@$EPOCH" -czf "$TARBALL" -C "$DIST" "$NAME"
(cd "$DIST" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")

echo
echo "package: $TARBALL"
cat "$TARBALL.sha256"

if [ "$smoke" -eq 1 ]; then
    echo
    echo "== smoke test"
    "$ROOT/tools/smoke_package.sh" "$TARBALL"
fi
