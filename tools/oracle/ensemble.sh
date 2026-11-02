#!/bin/bash
# ensemble.sh <label> <arena.py> <botA> <botB> [seconds]
#
# One fight as an ensemble (docs/29 section 12ag): `fidtrace match` with
# FIDTRACE_WARMUP = W0..W1 (default 0..47), seven at a time, into
# $OUT/<label>/ (h<w>.jsonl: every priced hit, d<w>.jsonl: the applied ones,
# t<w>.tsv: the poses, log<w>.txt). Arena and bots are relative to GAME.
#
#   GAME=/path/to/a/scratch/install OUT=/abs/scratch/dir \
#     tools/oracle/ensemble.sh box box.py Team0/Bot0.bot Team0/Bot1.bot 120
#   python3 -I tools/oracle/ensemble_report.py $OUT/box
#
# PERTURB=FIDTRACE_JITTER keeps the fall at the bell (the warmup starts the robots
# at rest on the floor, docs/29 section 12ai); FIDTRACE_UP=1 FIDTRACE_COM=1 in the
# environment put the chassis' up.y and centre of mass in the traces.
# FIRE=1 also writes f<w>.txt, every change of a burst device's cycle (docs/29 12ap).
# GAME is only read; OUT must be absolute (fidtrace changes directory). The
# bots of the four ensembles of the docs: box Team0/Bot0 Team0/Bot1 (Scout,
# ALARM); compressor.py Team0/Bot2 Team8/Bot2 (Sentinel, Wide Load); box.py
# Team10/Bot1 Team8/Bot0 (Razor, FlapJack); octagon.py Team0/Bot0 Team0/Bot1.
set -e
HERE=$(cd "$(dirname "$0")/../.." && pwd)
GAME=${GAME:-$HERE/local/game}
BIN=${BIN:-$HERE/engine/target/release/examples/fidtrace}
OUT=${OUT:?set OUT to an absolute scratch directory}
L=$1; A=$2; B1=$3; B2=$4; T=${5:-120}
O=$OUT/$L; mkdir -p "$O"
run() {
  w=$1
  env "${PERTURB:-FIDTRACE_WARMUP}=$w" ${FIRE:+FIDTRACE_FIRE=$O/f$w.txt} FIDTRACE_HITS=$O/h$w.jsonl "$BIN" match "$GAME/Arenas/$A" "$GAME/AI/$B1" "$GAME/AI/$B2" "$T" "$O/t$w.tsv" "$O/d$w.jsonl" > "$O/log$w.txt" 2>&1
}
export -f run; export O BIN GAME A B1 B2 T PERTURB FIRE
seq "${W0:-0}" "${W1:-47}" | xargs -P "${JOBS:-7}" -I{} bash -c 'run {}'
