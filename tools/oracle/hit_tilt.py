#!/usr/bin/env python3
"""hit_tilt.py: what a hit does to the robot's tilt (docs/29 12am).

  hit_tilt.py live <alarmfight-*.jsonl ...>   (B rows every substep: the Scout's chassis is the
                                               body with mass 163.49, ALARM's 232.12)
  hit_tilt.py ours <ensemble dir> [n]         (FIDTRACE_UP=1: t<w>.tsv up.y every 0.1 s, h<w>.jsonl)
For every priced hit of >= MIN hp (default 30) on the Scout by ALARM (att 1 / vic 0): the Scout's up.y
at the hit and its lowest up.y within 0.7 s after it, then the share of hits that turn it over
(up.y < 0 at some point).
"""
import ast, glob, json, os, sys

MIN = float(os.environ.get("MIN", "30"))
H = 0.7

def live(paths):
    out = []
    for p in paths:
        t = 0.0
        up, hits = [], []
        for line in open(p):
            r = json.loads(line)
            if r[0] == "A":
                t += r[2]
            elif r[0] == "B" and abs(r[3] - 163.49465942382812) < 1e-3:
                up.append((t, r[7][4]))
            elif r[0] == "H" and r[2] == 1 and r[4] == 0 and r[6] >= MIN:
                hits.append((t, r[6]))
        out.append((hits, up))
    return out

def ours(d, n):
    out = []
    for w in range(n):
        up = []
        for line in open(f"{d}/t{w}.tsv").read().splitlines()[1:]:
            a, b = line.split("\t", 1)
            try:
                rows = ast.literal_eval(b)
            except (ValueError, SyntaxError):
                continue
            up.append((float(a), rows[0][5]))
        hits = []
        for line in open(f"{d}/h{w}.jsonl"):
            h = json.loads(line)
            if h["att"] == 1 and h["vic"] == 0 and h["amount"] >= MIN and h["comp"] != "":
                hits.append((h["t"], h["amount"]))
        out.append((hits, up))
    return out

def report(fights, label):
    n = flips = low = 0
    amounts = []
    for hits, up in fights:
        for t, a in hits:
            before = [u for tt, u in up if t - 0.1 <= tt <= t]
            if not before or before[-1] < 0.5:
                continue  # only a robot that was upright
            after = [u for tt, u in up if t <= tt <= t + H]
            if not after:
                continue
            n += 1
            m = min(after)
            flips += m < 0
            low += m < 0.5
            amounts.append((a, m))
    print(f"{label}: {len(fights)} fights, {n} hits >= {MIN:g} hp on an upright Scout; lowest up.y within {H} s: < 0.5 in {low}, < 0 in {flips}")
    return amounts

if __name__ == "__main__":
    if sys.argv[1] == "live":
        report(live(sys.argv[2:]), "live")
    else:
        report(ours(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 48), "ours")
