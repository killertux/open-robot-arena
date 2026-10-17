#!/bin/sh
# Install or uninstall the portable Open Robot Arena package.
#
#   ./install.sh              install for the current user
#   ./install.sh --system     install under /usr/local (run with sudo)
#   ./install.sh --prefix DIR install under DIR
#   ./install.sh --uninstall  remove an installation (game files are kept)
#
# The game is not installed or removed: it lives wherever the player put it,
# and is found through RA2_GAME_DIR, --game, or the documented search.

set -eu

APP=ra2-open-robot-arena

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)

mode=user
prefix=
action=install
while [ $# -gt 0 ]; do
    case $1 in
        --user) mode=user ;;
        --system) mode=system ;;
        --prefix)
            shift
            [ $# -gt 0 ] || { echo "install.sh: --prefix needs a directory" >&2; exit 2; }
            mode=prefix
            prefix=$1
            ;;
        --uninstall) action=uninstall ;;
        -h | --help)
            sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "install.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

case $mode in
    user)
        data=${XDG_DATA_HOME:-${HOME:-/nonexistent}/.local/share}
        appdir=$data/$APP/app
        bindir=${HOME:-/nonexistent}/.local/bin
        desktopdir=$data/applications
        icondir=$data/icons/hicolor
        ;;
    system)
        appdir=/usr/local/lib/$APP
        bindir=/usr/local/bin
        desktopdir=/usr/local/share/applications
        icondir=/usr/local/share/icons/hicolor
        ;;
    prefix)
        appdir=$prefix/lib/$APP
        bindir=$prefix/bin
        desktopdir=$prefix/share/applications
        icondir=$prefix/share/icons/hicolor
        ;;
esac

desktop=$desktopdir/$APP.desktop
icon_png=$icondir/256x256/apps/$APP.png
icon_svg=$icondir/scalable/apps/$APP.svg

refresh_caches() {
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q "$desktopdir" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t "$icondir" 2>/dev/null || true
    fi
}

if [ "$action" = uninstall ]; then
    rm -f "$bindir/$APP" "$desktop" "$icon_png" "$icon_svg"
    rm -rf "$appdir"
    refresh_caches
    echo "removed $appdir, $bindir/$APP, $desktop and the icon"
    echo "kept the game directory and any frames you wrote"
    exit 0
fi

if [ ! -f "$here/bin/ra2-shell" ]; then
    echo "install.sh: run this from the unpacked package (bin/ra2-shell not found)" >&2
    exit 2
fi

mkdir -p "$appdir" "$bindir" "$desktopdir" "$icondir/256x256/apps" "$icondir/scalable/apps"
rm -rf "$appdir/bin"
cp -R "$here/bin" "$appdir/bin"
for file in run.sh README.txt LICENSE-MIT LICENSE-APACHE NOTICE THIRD_PARTY.md; do
    [ -f "$here/$file" ] && cp "$here/$file" "$appdir/$file"
done
chmod 755 "$appdir/run.sh"

cp "$here/share/applications/$APP.desktop" "$desktop"
cp "$here/share/icons/hicolor/256x256/apps/$APP.png" "$icon_png"
cp "$here/share/icons/hicolor/scalable/apps/$APP.svg" "$icon_svg"

ln -sfn "$appdir/run.sh" "$bindir/$APP"

refresh_caches

echo "installed Open Robot Arena to $appdir"
echo "launcher: $bindir/$APP"
echo "desktop entry: $desktop"
echo
echo "Supply your own game directory when you run it:"
echo "  $APP --game \"/path/to/Robot Arena 2\""
case $mode in
    user) echo "or put it at ${XDG_DATA_HOME:-$HOME/.local/share}/$APP/game (for --user)." ;;
    system) echo "or put it at /usr/local/share/$APP/game (for --system)." ;;
    prefix) echo "or put it at $prefix/share/$APP/game (for --prefix)." ;;
esac
echo "The game files are never copied into the installation."
