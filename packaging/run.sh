#!/bin/sh
# Open Robot Arena — portable launcher (Linux).
#
# Finds the game installation, checks the CPython 3.12 runtime the binaries
# link against, and runs bin/ra2-shell. `--game DIR` wins; otherwise
# RA2_GAME_DIR and a documented search are used. The game is NOT part of this
# package: only the files the shell boots from are checked here (RA2.cfz, UI/,
# Scripts/), and a missing install is reported with those names.
#
# This file is also the installed launcher: install.sh symlinks it as
# `ra2-open-robot-arena` in $PREFIX/bin, and the symlink is resolved so `bin/`
# is still found.

set -eu

APP=ra2-open-robot-arena

# The directory holding this script, with symlinks resolved.
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
BIN=$ROOT/bin/ra2-shell

if [ ! -x "$BIN" ]; then
    echo "$APP: $BIN is missing or not executable; unpack the package again." >&2
    exit 2
fi

usage() {
    cat <<EOF
Open Robot Arena launcher

  ./run.sh [options]                     run the shell (botlab route by default)
  ./run.sh --game DIR --route exhibition run another shipped route
  ./run.sh --check-python                report the CPython 3.12 runtime check
  ./run.sh --help                        this text and ra2-shell's options

Game directory, in order:
  1. --game DIR
  2. \$RA2_GAME_DIR
  3. \$XDG_DATA_HOME/$APP/game  (default \$HOME/.local/share/$APP/game)
  4. game/ beside this launcher, or ../game when installed
  5. ./local/game upwards from the current directory (a source checkout)
  6. /usr/local/share/$APP/game, /usr/share/$APP/game, /opt/ra2/game

A game directory contains RA2.cfz, UI/ and Scripts/; the game is not
included in this package and must be supplied by the player. The other
shipped binaries (ra2, ra2-render, ra2-sim, ra2-duel) are in bin/.

Without --route or --script, the launcher runs the botlab route and writes
its frames to:
  \$XDG_STATE_HOME/$APP/frames  (default \$HOME/.local/state/$APP/frames)
EOF
}

# The shared library is a DT_NEEDED entry: the binaries cannot even start
# without it, and `--no-python` does not change that (the dynamic loader runs
# before main()).
python_lib_ok() {
    command -v ldd >/dev/null 2>&1 || return 0
    line=$(ldd "$BIN" 2>/dev/null | grep 'libpython3\.12\.so\.1\.0' || true)
    [ -n "$line" ] || return 1
    case $line in
        *"not found"*) return 1 ;;
    esac
    return 0
}

# The 3.12 standard library is what the embedded interpreter imports. A
# python3.12 executable is the usual proof; PYTHONHOME and the common lib
# directories cover a relocatable 3.12 install (e.g. pyenv + LD_LIBRARY_PATH).
python_stdlib_ok() {
    if command -v python3.12 >/dev/null 2>&1 &&
        python3.12 -c 'import sys' >/dev/null 2>&1; then
        return 0
    fi
    if command -v python3 >/dev/null 2>&1 &&
        python3 -c 'import sys; raise SystemExit(0 if sys.version_info[:2] == (3, 12) else 1)' >/dev/null 2>&1; then
        return 0
    fi
    for dir in "${PYTHONHOME:-}/lib/python3.12" \
        /usr/lib/python3.12 /usr/local/lib/python3.12 /usr/lib64/python3.12; do
        [ -f "$dir/os.py" ] && return 0
    done
    return 1
}

python_description() {
    if command -v python3.12 >/dev/null 2>&1; then
        python3.12 -c 'import sys; print("python3.12 " + sys.version.split()[0])' 2>/dev/null && return 0
    fi
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import sys; print("python3 %s.%s.%s" % sys.version_info[:3])' 2>/dev/null && return 0
    fi
    echo "no python3.12 on PATH"
}

check_python() {
    lib=ok
    if ! python_lib_ok; then lib=missing; fi
    stdlib=ok
    if ! python_stdlib_ok; then stdlib=missing; fi
    if [ "$lib" = ok ] && [ "$stdlib" = ok ]; then
        return 0
    fi
    {
        echo "$APP: this build embeds CPython 3.12 and needs the system runtime."
        echo "  libpython3.12.so.1.0: $lib"
        echo "  CPython 3.12 stdlib:  $stdlib"
        echo
        echo "The interpreter is not bundled. Install the 3.12 runtime, for example:"
        echo "  Debian/Ubuntu: sudo apt install python3.12 libpython3.12"
        echo "  Fedora:        sudo dnf install python3.12"
        echo "  Arch:          the rolling python is newer; use a 3.12 build and set"
        echo "                 LD_LIBRARY_PATH and PYTHONHOME to it."
        echo "Check with: ldd \"$BIN\" | grep libpython"
    } >&2
    return 1
}

print_python_check() {
    echo "launcher: $ROOT"
    printf 'CPython: '
    python_description
    if python_lib_ok; then
        echo "libpython3.12.so.1.0: found"
    else
        echo "libpython3.12.so.1.0: MISSING"
    fi
    if python_stdlib_ok; then
        echo "CPython 3.12 stdlib: found"
    else
        echo "CPython 3.12 stdlib: MISSING"
    fi
}

# Arguments we inspect; everything else is the shell's.
game_arg=
out_arg=
has_route=0
want_help=0
want_check=0
prev=
for arg in "$@"; do
    case $prev in
        --game) game_arg=$arg ;;
        --out) out_arg=$arg ;;
    esac
    case $arg in
        --game | --out) ;;
        --route | --script) has_route=1 ;;
        -h | --help) want_help=1 ;;
        --check-python) want_check=1 ;;
    esac
    prev=$arg
done

if [ "$want_check" -eq 1 ]; then
    print_python_check
    python_lib_ok && python_stdlib_ok
    exit $?
fi

if [ "$want_help" -eq 1 ]; then
    usage
    echo
    exec "$BIN" --help
fi

is_game() {
    [ -d "$1" ] && [ -f "$1/RA2.cfz" ] && [ -d "$1/UI" ] && [ -d "$1/Scripts" ]
}

missing_files() {
    for name in RA2.cfz UI Scripts; do
        if [ ! -e "$1/$name" ]; then printf '  %s\n' "$name"; fi
    done
}

checked=
try_game() {
    checked="$checked
  $1"
    if is_game "$1"; then
        GAME=$1
        return 0
    fi
    return 1
}

data=${XDG_DATA_HOME:-${HOME:-/nonexistent}/.local/share}
state=${XDG_STATE_HOME:-${HOME:-/nonexistent}/.local/state}

GAME=
if [ -n "$game_arg" ]; then
    if ! is_game "$game_arg"; then
        {
            echo "$APP: $game_arg is not a Robot Arena 2 installation."
            echo "It must contain:"
            missing_files "$game_arg"
            echo "The game is not included with this package; supply your own copy"
            echo "and pass the directory that holds RA2.cfz, UI/ and Scripts/."
        } >&2
        exit 2
    fi
    GAME=$game_arg
elif [ -n "${RA2_GAME_DIR:-}" ]; then
    if ! is_game "$RA2_GAME_DIR"; then
        {
            echo "$APP: RA2_GAME_DIR=$RA2_GAME_DIR is not a Robot Arena 2 installation."
            echo "It must contain:"
            missing_files "$RA2_GAME_DIR"
            echo "Unset RA2_GAME_DIR or point it at the directory that holds RA2.cfz, UI/ and Scripts/."
        } >&2
        exit 2
    fi
    GAME=$RA2_GAME_DIR
else
    # `try_game` records every path in `checked`, so the error can list the
    # whole search instead of only the last attempt.
    if try_game "$data/$APP/game"; then :; fi
    if [ -z "$GAME" ] && try_game "$ROOT/game"; then :; fi
    if [ -z "$GAME" ] && try_game "$ROOT/../game"; then :; fi
    if [ -z "$GAME" ] && try_game "$ROOT/../../share/$APP/game"; then :; fi
    if [ -z "$GAME" ]; then
        dir=$(pwd -P)
        while :; do
            if try_game "$dir/local/game"; then break; fi
            parent=$(dirname -- "$dir")
            [ "$parent" = "$dir" ] && break
            dir=$parent
        done
    fi
    if [ -z "$GAME" ] && try_game "/usr/local/share/$APP/game"; then :; fi
    if [ -z "$GAME" ] && try_game "/usr/share/$APP/game"; then :; fi
    if [ -z "$GAME" ] && try_game "/opt/ra2/game"; then :; fi

    if [ -z "$GAME" ]; then
        {
            echo "$APP: no Robot Arena 2 installation found."
            echo
            echo "The game is not included with this package. A game directory must contain:"
            echo "  RA2.cfz"
            echo "  UI/"
            echo "  Scripts/"
            echo
            echo "Looked in:$checked"
            echo
            echo "Provide one with either:"
            echo "  RA2_GAME_DIR=\"/path/to/Robot Arena 2\" ./run.sh"
            echo "  ./run.sh --game \"/path/to/Robot Arena 2\""
        } >&2
        exit 2
    fi
fi

check_python

OUT=${out_arg:-$state/$APP/frames}
if [ "$want_help" -eq 0 ]; then
    echo "$APP: game directory: $GAME"
fi
if [ "$has_route" -eq 0 ] && [ "$want_help" -eq 0 ]; then
    echo "$APP: no --route or --script, running the botlab route; frames go to $OUT"
fi

if [ -z "$out_arg" ] && [ "$has_route" -eq 0 ]; then
    exec "$BIN" --game "$GAME" --out "$OUT" --route botlab "$@"
elif [ -z "$out_arg" ]; then
    exec "$BIN" --game "$GAME" --out "$OUT" "$@"
elif [ "$has_route" -eq 0 ]; then
    exec "$BIN" --game "$GAME" --route botlab "$@"
else
    exec "$BIN" --game "$GAME" "$@"
fi
