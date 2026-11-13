#!/usr/bin/env python3
"""spin_rows.py capture.jsonl [steps]: per trial of a tipab-wheelspin capture (docs/29 12bi) every body's angular
velocity relative to the chassis (the first body of the `place` X row), per substep after the release, by the
last four digits of the body pointer (`B` rows: [B, seq, ptr, mass, com, vel, angvel, rot])."""
import json, math, sys
n = int(sys.argv[2]) if len(sys.argv) > 2 else 12
trial = None; chassis = None; steps = []; placed = False
def show():
    if trial is None: return
    print("trial", trial)
    for k, d in enumerate(steps[:n]):
        wc = d[chassis]
        print("%3d" % k, " ".join("%s:%.3f" % (p[-4:], math.dist(w, wc)) for p, w in d.items() if p != chassis),
              " chassis|w| %.3f" % math.hypot(*wc))
for l in open(sys.argv[1]):
    r = json.loads(l)
    if r[0] == "TRIAL":
        show(); trial = r[2].get("label"); steps = []; chassis = None; placed = False
    elif r[0] == "X" and r[2] == "place":
        chassis = r[3][0][0]; placed = True
        steps = [{b[0]: b[4] for b in r[3]}]
    elif r[0] == "B" and placed:
        if len(steps) == 1 or steps[-1].get("_seq") != r[1]:
            steps.append({"_seq": r[1]}) if False else None
        # group by seq
        if not hasattr(show, "last") or show.last != r[1]:
            steps.append({}); show.last = r[1]
        steps[-1][r[2]] = r[6]
show()
