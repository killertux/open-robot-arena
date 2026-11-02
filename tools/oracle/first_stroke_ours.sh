#!/bin/bash
# first_stroke_ours.sh <abs out dir> [seconds]: the first seconds of the Box fight (Scout
# Team0/Bot0 v ALARM Team0/Bot1) as 48 warmups with every body's pose per step
# (FIDTRACE_BODY, b<w>.txt) and every priced hit (FIDTRACE_HITS, h<w>.jsonl), for
# first_stroke.py (docs/29 section 12as). GAME (default local/game) is only read;
# RA2_TAGS=0 gives the nearest-box rule.
set -e
HERE=$(cd "$(dirname "$0")/../.." && pwd)
GAME=${GAME:-$HERE/local/game}
BIN=${BIN:-$HERE/engine/target/release/examples/fidtrace}
OUT=${1:?abs out dir}; T=${2:-7}
mkdir -p "$OUT"
run() {
  w=$1
  FIDTRACE_WARMUP=$w FIDTRACE_BODY=$OUT/b$w.txt FIDTRACE_HITS=$OUT/h$w.jsonl "$BIN" match \
    "$GAME/Arenas/box.py" "$GAME/AI/Team0/Bot0.bot" "$GAME/AI/Team0/Bot1.bot" "$T" "$OUT/t$w.tsv" "$OUT/d$w.jsonl" > /dev/null 2>&1
}
export -f run; export OUT GAME BIN T
seq 0 47 | xargs -P "${JOBS:-6}" -I{} bash -c 'run {}'
