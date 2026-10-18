#!/bin/sh
# Render our version of every captured original screen at the same resolution
# and measure the pair with tools/oracle/compare.py.
#
#   tools/capture/ab.sh
#
# Reference frames live in <local>/oracle/original, our renders in
# <local>/oracle/ours, and the JSON/diff sheets in <local>/oracle/diff.  `<local>`
# is the main worktree's ignored `local/` by default; RA2_ORACLE_LOCAL overrides
# it (useful from a linked worktree, whose own `local/` is not where the game or
# the captures live).
#
# The original screens this harness could reach are the two `mainmenu` sizes
# (docs/22-windowed.md §4).  A missing fixture for another id is reported and
# skipped, never invented.  The canonical A/B size is 1024x768; the 1280x1024
# menu is the shipped-config reference and carries the 1280x1005-vs-1280x1024
# caveat from docs/12-oracle.md §4.
#
# Output columns: id, size, MAE, pointer-masked MAE, blocks > 20, largest blocks.
# The same lines are what docs/22-windowed.md's table records.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd -P)
ENGINE=$ROOT/engine

MAIN_ROOT=$ROOT
if COMMON=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
    MAIN_ROOT=$(dirname -- "$COMMON")
fi
LOCAL=${RA2_ORACLE_LOCAL:-$MAIN_ROOT/local}
GAME=${RA2_GAME:-$LOCAL/game}
ORIG=$LOCAL/oracle/original
OURS=$LOCAL/oracle/ours
DIFF=$LOCAL/oracle/diff

[ -d "$GAME/UI" ] || { echo "ab: no game data at $GAME" >&2; exit 2; }
[ -d "$ORIG" ] || { echo "ab: skip: no original fixtures at $ORIG" >&2; exit 2; }
mkdir -p -- "$OURS" "$DIFF"

UISHOT=$ENGINE/target/release/examples/uishot
if [ ! -x "$UISHOT" ]; then
    (cd "$ENGINE" && cargo build --release -q -p ra2-render --example uishot)
fi

# screen for each fixture id; only screens the shell can render faithfully from
# a single .gib are listed.  A state composed by the shell (arena picker, bot
# lab, match HUD) needs its headless runner and is not guessed here.
screen_for() {
    case $1 in
        mainmenu) echo UI/mainmenu/mainmenu.gib ;;
        *) return 1 ;;
    esac
}

printf '%-22s %-10s %8s %8s %6s  %s\n' id size MAE masked '>20' 'largest 16x16 blocks (x,y)=MAE'
found=0
seen=
# Newest sequence first: `ls -r` means a later capture of the same id+size wins.
for ref in $(ls -r "$ORIG"/*.png 2>/dev/null); do
    [ -f "$ref" ] || continue
    base=$(basename -- "$ref" .png)
    id=${base%%__*}
    rest=${base#*__}
    size=${rest%%__*}
    width=${size%x*}
    height=${size#*x}
    case $width$height in *[!0-9]*) echo "ab: skip $base (unparsable size)" >&2; continue ;; esac
    key=${id}__${size}
    case " $seen " in *" $key "*) continue ;; esac
    seen="$seen $key"
    if ! screen=$(screen_for "$id"); then
        echo "ab: skip $base (no single-.gib render for $id)" >&2
        continue
    fi
    ours=$OURS/${id}__${width}x${height}.png
    diff=$DIFF/${id}__${width}x${height}__diff.png
    json=$DIFF/${id}__${width}x${height}__diff.json
    "$UISHOT" "$GAME/$screen" "$ours" "$width" "$height" >/dev/null
    pointer_args=
    if [ -f "${ref%.png}.pointer" ]; then
        pointer_args="--pointer-rect $(cat "${ref%.png}.pointer")"
    elif [ -f "${ref%.png}.nopointer" ]; then
        pointer_args="--pointer-scan off"
    fi
    # shellcheck disable=SC2086
    if ! python3 "$ROOT/tools/oracle/compare.py" "$ours" "$ref" "$diff" \
        $pointer_args >"$json" 2>"$DIFF/${id}__${width}x${height}__compare.err"; then
        echo "ab: compare failed for $base; see $DIFF/${id}__${width}x${height}__compare.err" >&2
        continue
    fi
    row=$(python3 "$SCRIPT_DIR/summarize_diff.py" "$json")
    mae=$(printf '%s\n' "$row" | cut -f1)
    masked=$(printf '%s\n' "$row" | cut -f2)
    over=$(printf '%s\n' "$row" | cut -f3)
    regions=$(printf '%s\n' "$row" | cut -f4)
    printf '%-22s %-10s %8s %8s %6s  %s\n' "$id" "$size" "$mae" "$masked" "$over" "$regions"
    found=$((found + 1))
done

[ "$found" -gt 0 ] || { echo "ab: no comparable fixtures" >&2; exit 2; }
echo "ab: $found pair(s); JSON and diff sheets in $DIFF" >&2
