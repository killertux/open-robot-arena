#!/bin/bash
# rock_probe.sh [ENV=VAL ...]: the Scout pitched -90 degrees and spun at -3 rad/s about its side axis; prints the extremes of the chassis'
# lean (the up axis' height) in the first 1.2 s: live (tipab-scout-tail-wscan-box, 'pitch -90 w -3') swings back to -0.144 at step 8 and
# forward to +0.103 at step 43, ours to -0.120 (step 7) and +0.031 (step 24) (docs/29 12bd).
cd "$(dirname "$0")/../../../engine" || exit 1
G=${GAME:-../local/game}
D=$(mktemp -d)
env LC_ALL=C TIPPROBE_ROBOT=0 TIPPROBE_COM_LIFT=0.3598 RA2_H17_DUMP=$D/d.jsonl "$@" ./target/release/examples/tipprobe \
  $G/Arenas/box.py $G/AI/Team0/Bot0.bot $G/AI/Team0/Bot1.bot self 1 0 0 -90 -3 4 > $D/o.txt 2>&1
python3 -I - $D <<'P'
import json, sys
rows, rel = [], False
for line in open(sys.argv[1] + "/d.jsonl"):
    if line.startswith('["X",0]'):
        rel = True
    if rel and line.startswith('["B"'):
        r = json.loads(line)
        if abs(r[3] - 163.49) < 0.1:
            rows.append(r)
ys = [r[7][4] for r in rows[:110]]
print([(i, round(ys[i], 3)) for i in range(1, len(ys) - 1)
       if ((ys[i] > ys[i - 1] and ys[i] >= ys[i + 1]) or (ys[i] < ys[i - 1] and ys[i] <= ys[i + 1])) and abs(ys[i]) > 0.01][:8])
P
rm -rf $D
