#!/bin/sh
# Render the canonical screen and compare it with the captured original.
#
#   tools/oracle/check.sh
#
# This is the oracle regression gate, deliberately separate from
# `tools/check.sh`: the oracle needs the local game data and the uncommitted
# captured frames in `local/oracle/`, while CI has neither.  It exits 2 (a
# skip, not a failure) when the reference frame is absent.
#
# The threshold is the first measured 1024x768 main-menu number: 1.439 overall
# (pointer masked), rounded up to 1.45 for renderer/compiler noise.  Change it
# only with a new measurement and a note in docs/12-oracle.md.
#
# Environment:
#   RA2_ORACLE_THRESHOLD  override the ceiling (overall MAE, pointer masked)
#   RA2_ORACLE_LOCAL      use another local/ directory (default: main worktree)
#   RA2_GAME              the game install (default: resolved by render_ours.sh)

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd -P)

MAIN_ROOT=$ROOT
if COMMON=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
    MAIN_ROOT=$(dirname -- "$COMMON")
fi
LOCAL=${RA2_ORACLE_LOCAL:-$MAIN_ROOT/local}

REF=$LOCAL/oracle/original/mainmenu__1024x768__000.png
OURS=$LOCAL/oracle/ours/mainmenu__1024x768.png
DIFF=$LOCAL/oracle/diff/mainmenu__1024x768__diff.png
THRESHOLD=${RA2_ORACLE_THRESHOLD:-1.45}

if [ ! -f "$REF" ]; then
    echo "check: skip: no reference frame at $REF" >&2
    echo "       Capture one (tools/oracle/README.md); CI has no game files." >&2
    exit 2
fi

mkdir -p -- "$(dirname -- "$OURS")" "$(dirname -- "$DIFF")"
# The screen defaults to UI/mainmenu/mainmenu.gib; render_ours.sh resolves the
# game root and prints the output path as its last stdout line. Keep the whole
# substitution (not a pipe) so `set -e` still sees a failed render.
RENDER_LOG=$("$SCRIPT_DIR/render_ours.sh" "" "$OURS" 1024 768)
RENDERED=$(printf '%s\n' "$RENDER_LOG" | tail -n 1)

python3 "$SCRIPT_DIR/compare.py" "$RENDERED" "$REF" "$DIFF" \
    --check --threshold "$THRESHOLD"
