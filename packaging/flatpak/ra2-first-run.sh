#!/bin/sh
# Open Robot Arena — Flatpak first-run wrapper.
#
# Installed as /app/bin/ra2-open-robot-arena (a symlink to this file under
# /app/lib/ra2-open-robot-arena).  It asks for the player's Robot Arena 2
# directory once, remembers it, and then hands over to run.sh with the same
# --game argument the portable package uses.  The game is never copied into
# the sandbox or into the app.
#
# Why a picker at all: Flatpak has no default location for the game, and a
# sandbox cannot browse the host filesystem.  zenity talks to the XDG desktop
# portal, so the chosen directory is exported in the document portal and is
# readable by the client.  Directories under $HOME also work through the
# manifest's --filesystem=home:ro.  A directory elsewhere can be granted with:
#
#   flatpak override --user --filesystem="/path/to/game:ro" \
#       org.openrobotarena.OpenRobotArena
#
# If zenity is unavailable (the runtime does not ship it, or there is no
# display) this falls back to a terminal prompt.  With neither, it prints the
# exact override/run commands and exits 2.

set -eu

APP=ra2-open-robot-arena
APP_ID=org.openrobotarena.OpenRobotArena
CONFIG=${XDG_CONFIG_HOME:-${HOME:-/nonexistent}/.config}/$APP
SAVED=$CONFIG/game-dir

# The directory holding this script, with symlinks resolved (same rule as
# run.sh), so /app/bin/ra2-open-robot-arena finds /app/lib/.../run.sh.
resolve_root() {
    target=$0
    while [ -L "$target" ]; do
        link=$(readlink -- "$target")
        case $link in
            /*) target=$link ;;
            *) target=$(dirname -- "$target")/$link ;;
        esac
    done
    CDPATH= cd -- "$(dirname -- "$target")" && pwd -P
}

ROOT=$(resolve_root)
RUN=$ROOT/run.sh

usage() {
    cat <<EOF
Open Robot Arena (Flatpak)

  flatpak run $APP_ID                  open the client; asks for the game once
  flatpak run $APP_ID --game DIR       use DIR (the folder with RA2.cfz, UI/, Scripts/)
  flatpak run $APP_ID --check-python   report the CPython 3.12 runtime check
  flatpak run --command=ra2-shell $APP_ID --game DIR --route botlab --out DIR

The game directory is remembered in
\$XDG_CONFIG_HOME/$APP/game-dir (on the host, inside
~/.var/app/org.openrobotarena.OpenRobotArena/).  It is read in place and is
never bundled.
EOF
}

is_game() {
    [ -d "$1" ] && [ -f "$1/RA2.cfz" ] && [ -d "$1/UI" ] && [ -d "$1/Scripts" ]
}

missing_files() {
    for name in RA2.cfz UI Scripts; do
        if [ ! -e "$1/$name" ]; then printf '  %s\n' "$name"; fi
    done
}

game_arg=
want_help=0
want_check=0
headless=0
prev=
for arg in "$@"; do
    case $prev in
        --game) game_arg=$arg ;;
    esac
    case $arg in
        --game | --out) ;;
        -h | --help) want_help=1 ;;
        --check-python) want_check=1 ;;
        --offscreen | --match | --build | --hq) headless=1 ;;
    esac
    prev=$arg
done

if [ "$want_help" -eq 1 ]; then
    usage
    echo
    exec "$RUN" --help
fi

# run.sh handles these before it looks for a game.
if [ "$want_check" -eq 1 ]; then
    exec "$RUN" "$@"
fi

if [ -n "$game_arg" ]; then
    if is_game "$game_arg"; then
        mkdir -p "$CONFIG" 2>/dev/null || true
        printf '%s\n' "$game_arg" >"$SAVED" 2>/dev/null || true
    fi
    exec "$RUN" "$@"
fi

if [ -n "${RA2_GAME_DIR:-}" ]; then
    exec "$RUN" "$@"
fi

if [ -f "$SAVED" ]; then
    saved=$(sed -n '1p' "$SAVED")
    if [ -n "$saved" ] && is_game "$saved"; then
        exec "$RUN" --game "$saved" "$@"
    fi
fi

# The launcher's own documented default; if it exists, no picker is needed.
data=${XDG_DATA_HOME:-${HOME:-/nonexistent}/.local/share}
if is_game "$data/$APP/game"; then
    exec "$RUN" "$@"
fi

# 0 = a path was chosen, 2 = the user cancelled zenity, 1 = no way to ask.
pick() {
    # A headless run has no business opening a dialog; `flatpak run` with
    # --offscreen/--match/--build/--hq and no game gets the text error below.
    if [ "$headless" -eq 0 ] &&
        { [ -n "${WAYLAND_DISPLAY:-}" ] || [ -n "${DISPLAY:-}" ]; }; then
        if command -v zenity >/dev/null 2>&1; then
            if zenity --file-selection --directory \
                --title="Select your Robot Arena 2 folder" 2>/dev/null; then
                return 0
            fi
            return 2
        fi
    fi
    if [ -t 0 ]; then
        printf 'Path to the folder that contains RA2.cfz, UI/ and Scripts/: ' >&2
        if IFS= read -r line && [ -n "$line" ]; then
            printf '%s\n' "$line"
            return 0
        fi
    fi
    return 1
}

game=
tries=0
while [ "$tries" -lt 3 ]; do
    tries=$((tries + 1))
    if picked=$(pick); then
        :
    else
        status=$?
        [ "$status" -eq 2 ] && exit 0
        break
    fi
    if is_game "$picked"; then
        game=$picked
        break
    fi
    {
        echo "$APP: $picked is not a Robot Arena 2 installation."
        echo "It must contain:"
        missing_files "$picked"
        echo "Choose the folder that directly holds RA2.cfz, UI/ and Scripts/"
        echo "(not its parent)."
    } >&2
done

if [ -z "$game" ]; then
    {
        echo "$APP: no Robot Arena 2 folder was chosen."
        echo
        echo "The game is not included. A game folder must contain:"
        echo "  RA2.cfz"
        echo "  UI/"
        echo "  Scripts/"
        echo
        echo "Pick it again, or supply it on the command line:"
        echo "  flatpak run --env=RA2_GAME_DIR=\"/path/to/Robot Arena 2\" $APP_ID"
        echo
        echo "A folder outside \$HOME must also be granted to the sandbox once:"
        echo "  flatpak override --user --filesystem=\"/path/to/Robot Arena 2:ro\" \\"
        echo "      org.openrobotarena.OpenRobotArena"
    } >&2
    exit 2
fi

mkdir -p "$CONFIG"
printf '%s\n' "$game" >"$SAVED"
exec "$RUN" --game "$game" "$@"
