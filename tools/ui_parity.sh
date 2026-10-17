#!/bin/sh
# The two-renderer check: draw each shell screen twice and measure the gap.
#
#   tools/ui_parity.sh [--check] [--threshold N] [screen.gib ...]
#
# `ra2-render`'s `uishot` lays the screen out with `ra2ui`; `tools/ui_compose.py`
# reads the same `.gib` and lays it out itself. They share the game's art and
# the face pages, not the layout, so a rule only one of them implements shows up
# here. The default screens are the shell's baseline (main menu), the two
# match/layout screens the TextBox/baseline correction was measured on
# (`matchresults`, `statusmeters`), the workshop and HQ screens, the exhibition
# set-up, and four more whose labels sit in TextBoxes or whose groups are hidden
# (`gamewindow`, `result_cell`, `season_results`, `event_bracket`).
#
# Images and `report.json` go to `local/parity/` in the main worktree, beside
# the oracle's captures. `--check` exits 1 when a screen's MAE is above
# `--threshold` (default 0.75/255); without it every screen is measured and the
# command succeeds. This is deliberately **not** in `tools/check.sh`: the
# residual is GPU-versus-CPU glyph rasterisation, so a ceiling would encode the
# rasteriser rather than the layout, and it needs both a built `uishot` and the
# game. `tools/ui_parity.sh` is the review tool; `--check` makes it a gate for
# whoever wants one.
#
# With no game data it prints one line and exits 0, so a CI checkout without the
# game skips cleanly. `RA2_GAME`/`RA2_GAME_DIR` override the game root;
# `RA2_UISHOT`, `RA2_BIN` and `RA2_PARITY_OUT` override the binaries and the
# output directory.

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
MAIN_ROOT=$ROOT
if COMMON=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
    MAIN_ROOT=$(dirname -- "$COMMON")
fi

if [ -n "${RA2_GAME:-}" ]; then
    GAME=$RA2_GAME
elif [ -n "${RA2_GAME_DIR:-}" ]; then
    GAME=$RA2_GAME_DIR
elif [ -d "$ROOT/local/game/UI" ]; then
    GAME=$ROOT/local/game
elif [ -d "$MAIN_ROOT/local/game/UI" ]; then
    GAME=$MAIN_ROOT/local/game
else
    echo "ui_parity: no game data at local/game or RA2_GAME -- skipping"
    exit 0
fi

if [ ! -d "$GAME/UI" ]; then
    echo "ui_parity: no game data at $GAME/UI -- skipping"
    exit 0
fi

OUT=${RA2_PARITY_OUT:-$MAIN_ROOT/local/parity}
mkdir -p -- "$OUT"

find_bin() {
    # find_bin <name> <cargo package> <cargo target flag> <target subpath>
    name=$1
    package=$2
    flag=$3
    sub=$4
    if [ -n "${5:-}" ] && [ -x "$5" ]; then
        printf '%s\n' "$5"
        return 0
    fi
    for profile in debug release; do
        candidate=$ROOT/engine/target/$profile/$sub
        if [ -x "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    # Nothing built here yet: build the one piece, offline. The parity check is
    # a local, game-required tool, so this cost is paid on first use.
    ( cd "$ROOT/engine" && CARGO_NET_OFFLINE=true cargo build --offline -q "$flag" -p "$package" ) \
        >&2 || return 1
    for profile in debug release; do
        candidate=$ROOT/engine/target/$profile/$sub
        if [ -x "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    echo "ui_parity: built $name but cannot find it under engine/target" >&2
    return 1
}

UISHOT=$(find_bin uishot ra2-render --example examples/uishot "${RA2_UISHOT:-}") || exit 2
RA2_BIN=$(find_bin ra2 ra2-cli --bin ra2 "${RA2_BIN:-}") || exit 2
export RA2_BIN

exec python3 "$ROOT/tools/ui_parity.py" \
    --game "$GAME" \
    --uishot "$UISHOT" \
    --out "$OUT" \
    "$@"
