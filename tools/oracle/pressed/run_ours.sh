#!/bin/bash
# run_ours.sh <warmup> <tag>: one Compressor fight of ours with the step-level dump (RA2_H17_DUMP) reduced
# to $PRESSED_WORK/ours/<tag>/{s,e}<w>.pkl (docs/29 12ay); PRESSED_GAME a scratch install; env passes through.
w=$1; tag=$2; S=${PRESSED_WORK:?scratch work dir}; G=${PRESSED_GAME:?scratch install}
TOOLS=$(dirname $(readlink -f $0))
BIN=${BIN:-$HOME/Projects/ora-wt/pressed/engine/target/release/examples/fidtrace}
mkdir -p $S/ours/$tag
RA2_AI_TRACE=1 FIDTRACE_WARMUP=$w FIDTRACE_UP=1 RA2_H17_DUMP=$S/ours/$tag/d$w.jsonl $BIN match $G/Arenas/compressor.py $G/AI/Team0/Bot2.bot $G/AI/Team8/Bot2.bot ${SECS:-125} $S/ours/$tag/t$w.tsv $S/ours/$tag/dd$w.jsonl > $S/ours/$tag/log$w.txt 2>&1
python3 $TOOLS/rp_reduce.py ours $S/ours/$tag/d$w.jsonl $S/ours/$tag/s$w.pkl && python3 $TOOLS/rp_reduce2.py ours $S/ours/$tag/d$w.jsonl $S/ours/$tag/e$w.pkl && rm $S/ours/$tag/d$w.jsonl
