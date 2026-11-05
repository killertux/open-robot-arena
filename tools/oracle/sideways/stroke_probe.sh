#!/bin/bash
# stroke_probe.sh [ENV=VAL ...]: the Scout pitched -90 degrees (on its tail), its fork fired 0.7 s after the release; prints the chassis'
# angular velocity about its side axis at the steps around the fork's stop and the class of the end (docs/29 12bd). ARENA (default box).
# The live trials (hazards off) read 1.30 1.07 0.91 0.98 1.05 1.13 after the stop and the Scout topples; ours with the box's hazards
# on (its controllers after the solver, 12ba) keeps standing, with RA2_HAZARDS=0 (the live session's list) or on the Octagon it topples.
cd "$(dirname "$0")/../../../engine" || exit 1
G=${GAME:-../local/game}
D=$(mktemp -d)
env LC_ALL=C TIPPROBE_ROBOT=0 TIPPROBE_COM_LIFT=0.3598 RA2_H17_DUMP=$D/d.jsonl "$@" ./target/release/examples/tipprobe \
  $G/Arenas/${ARENA:-box}.py $G/AI/Team0/Bot0.bot $G/AI/Team0/Bot1.bot self 1 0 0 -90 0 6 0.0 0.7 > $D/o.txt 2>&1
python3 -I - $D <<'P'
import json, sys
rows = []
for line in open(sys.argv[1] + "/d.jsonl"):
    if line.startswith('["B"'):
        r = json.loads(line)
        if abs(r[3] - 163.49) < 0.1 or abs(r[3] - 26) < 0.5:
            rows.append(r)
# group per step by the chassis row; the stroke is the first step after 120 where the fork (26 kg) spins faster than 10 rad/s
g, cur = [], None
for r in rows:
    if abs(r[3] - 163.49) < 0.1:
        if cur:
            g.append(cur)
        cur = {}
    if cur is not None:
        cur[round(r[3])] = r
g.append(cur)
i0 = next(i for i, c in enumerate(g) if i > 120 and 26 in c and sum(abs(x) for x in c[26][6]) > 10)
w = []
for k in range(-1, 9):
    b = g[i0 + k][163]
    m = b[7]
    w.append("%.2f" % sum(m[i] * b[6][i] for i in range(3)))
b = g[-2][163]
print("w(x) steps -1..8:", " ".join(w), "| end up.y %.2f forward.y %.2f" % (b[7][4], b[7][7]))
P
rm -rf $D
