#!/bin/sh
# One command, offline: everything this project trusts.
#
#   ./tools/check.sh
#
# It runs, in this order, and stops at the first failure:
#
#   1. cargo fmt --all -- --check
#   2. cargo clippy --workspace --offline --all-targets -- -D warnings
#   3. cargo test --release --offline --workspace
#   4. cargo build --release --offline
#   5. tools/package.sh --no-smoke   (stage the portable Linux tarball)
#   6. tools/smoke_package.sh        (unpack it and use it as a player would)
#
# The smoke test is the headless render check: it walks the botlab route
# (7 frames), the exhibition route through the launcher (3 frames), and
# compares the boot frame byte for byte with an independent `uishot` render of
# mainmenu.gib. It is reused rather than duplicated here.
#
# No network: cargo runs --offline and CARGO_NET_OFFLINE=true is exported so
# build scripts (oxijolt's optional prebuilt archive) do not fetch either. The
# game is never read from the package; local/game (or RA2_GAME_DIR) is read in
# place by the smoke test. The game must be present: the corpus tests skip
# without it and the smoke cannot run.

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$ROOT"

export CARGO_NET_OFFLINE=true

if [ ! -d "$ROOT/local/game" ] && [ -z "${RA2_GAME_DIR:-}" ]; then
    echo "check: no game data at local/game and RA2_GAME_DIR is not set." >&2
    echo "       The corpus tests skip without it and the smoke test cannot run." >&2
    echo "       This project ships no game files; point RA2_GAME_DIR at your install." >&2
    exit 2
fi

TMP=$(mktemp -d "${TMPDIR:-/tmp}/ra2-check.XXXXXX")
stage="setup"
tests=0
tests_failed=0
botlab_frames=0
exhibition_frames=0
smoke_state="not run"
done=0

finish() {
    rc=$?
    if [ "$done" -eq 1 ]; then
        rm -rf "$TMP"
        return 0
    fi
    echo
    echo "check: FAIL at: $stage"
    echo "check: ${tests} tests passed, ${tests_failed} failed; ${botlab_frames} botlab + ${exhibition_frames} exhibition frames written; package smoke ${smoke_state}"
    rm -rf "$TMP"
    # `set -e` already made the failing command non-zero; keep a non-zero exit.
    [ "$rc" -ne 0 ] || rc=1
    exit "$rc"
}
trap finish EXIT

# Run one stage, keeping its output for the summary. A failure prints the whole
# log and lets the EXIT trap name the stage.
run() {
    stage=$1
    shift
    echo "== $stage"
    log=$TMP/$(printf '%s' "$stage" | tr ' /' '__').log
    if "$@" >"$log" 2>&1; then
        cat "$log"
    else
        cat "$log" >&2
        exit 1
    fi
}

run "fmt" sh -c 'cd engine && cargo fmt --all -- --check'
run "clippy" sh -c 'cd engine && cargo clippy --workspace --offline --all-targets -- -D warnings'
run "tests" sh -c 'cd engine && cargo test --release --offline --workspace'
test_result=$(sed -n 's/^test result: .* \([0-9][0-9]*\) passed; \([0-9][0-9]*\) failed.*/\1 \2/p' \
    "$TMP/tests.log" | awk '{ p += $1; f += $2 } END { print p + 0, f + 0 }')
tests=${test_result% *}
tests_failed=${test_result#* }
run "build" sh -c 'cd engine && cargo build --release --offline'
run "package" tools/package.sh --no-smoke
run "package smoke" tools/smoke_package.sh

frames=$(sed -n 's/^smoke: PASS.* \([0-9][0-9]*\) botlab frames, \([0-9][0-9]*\) exhibition frames.*/\1 \2/p' \
    "$TMP/package_smoke.log" | tail -1)
if [ -z "$frames" ]; then
    stage="package smoke summary"
    echo "check: the smoke passed but its summary line could not be read" >&2
    exit 1
fi
botlab_frames=${frames% *}
exhibition_frames=${frames#* }
smoke_state=ok

done=1
echo
echo "check: PASS — ${tests} tests passed, ${tests_failed} failed; ${botlab_frames} botlab + ${exhibition_frames} exhibition frames written; boot frame == independent uishot render of mainmenu.gib; package smoke ok"
