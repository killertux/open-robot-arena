#!/bin/sh
# Render one of our `.gib` screens at the original's resolution.
#
#   tools/oracle/render_ours.sh [screen.gib] [out.png] [w] [h]
#
# With no arguments: UI/mainmenu/mainmenu.gib at 1024x768, written to
# local/oracle/ours/mainmenu__1024x768.png.  `screen.gib` may be absolute, or
# relative to the game root (`UI/...`).  If `out.png` is a directory the default
# filename is used inside it.
#
# The repository root is resolved from this script's own path, so the checkout
# and a git worktree both work.  Game data is never committed: the game root is
# `RA2_GAME` if set, else `<repo>/local/game`, else the main worktree's
# `local/game` (which is where an ignored local/ directory actually lives when
# this runs from a worktree).  The output default likewise goes to the main
# worktree's local/oracle/ours/ so it sits beside the captured reference.
#
# It uses the same command the README documents (the cargo workspace is engine/):
#   cargo run -q -p ra2-render --example uishot -- <screen.gib> <out.png> <w> <h>
# The final stdout line is the absolute output path, so a wrapper can capture it.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd -P)

die() {
    echo "render_ours: $*" >&2
    exit 2
}

SCREEN=${1:-}
OUT=${2:-}
WIDTH=${3:-1024}
HEIGHT=${4:-768}

case $WIDTH in *[!0-9]*|'') die "width must be a positive integer: $WIDTH" ;; esac
case $HEIGHT in *[!0-9]*|'') die "height must be a positive integer: $HEIGHT" ;; esac

# The main worktree holds the ignored local/ directory even when this script
# is run from a linked worktree.
MAIN_ROOT=$ROOT
if COMMON=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
    MAIN_ROOT=$(dirname -- "$COMMON")
fi

if [ -n "${RA2_GAME:-}" ]; then
    GAME=$RA2_GAME
elif [ -d "$ROOT/local/game/UI" ]; then
    GAME=$ROOT/local/game
elif [ -d "$MAIN_ROOT/local/game/UI" ]; then
    GAME=$MAIN_ROOT/local/game
else
    die "no game data: set RA2_GAME to a Robot Arena 2 install (needs UI/)"
fi

if [ -z "$SCREEN" ]; then
    SCREEN=$GAME/UI/mainmenu/mainmenu.gib
elif [ ! -f "$SCREEN" ] && [ -f "$GAME/$SCREEN" ]; then
    SCREEN=$GAME/$SCREEN
fi
[ -f "$SCREEN" ] || die "no such screen: $SCREEN"
SCREEN=$(CDPATH= cd -- "$(dirname -- "$SCREEN")" && pwd -P)/$(basename -- "$SCREEN")

ID=$(basename -- "$SCREEN")
ID=${ID%.gib}

LOCAL=${RA2_ORACLE_LOCAL:-$MAIN_ROOT/local}
DEFAULT_OUT=$LOCAL/oracle/ours/${ID}__${WIDTH}x${HEIGHT}.png
if [ -z "$OUT" ]; then
    OUT=$DEFAULT_OUT
elif [ -d "$OUT" ]; then
    OUT=$OUT/${ID}__${WIDTH}x${HEIGHT}.png
fi

OUT_DIR=$(dirname -- "$OUT")
mkdir -p -- "$OUT_DIR"
# Absolute before the cd below: a relative out.png must stay where the caller
# asked for it, not land in engine/.
OUT_DIR=$(CDPATH= cd -- "$OUT_DIR" && pwd -P)
OUT=$OUT_DIR/$(basename -- "$OUT")

cd "$ROOT/engine"
cargo run -q -p ra2-render --example uishot -- "$SCREEN" "$OUT" "$WIDTH" "$HEIGHT"
printf '%s\n' "$OUT"
