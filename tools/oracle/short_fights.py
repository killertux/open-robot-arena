#!/usr/bin/env python3
"""short_fights.py <pair> <ensemble dir of that pair>: how the Box / Octagon fights end, bell-primed live against ours (docs/29 12bj).

Each decided fight is put in a class by the robot that went out (live: the `W` row's loser; ours: the chassis' hit points <= 0, else the robot
that moved least) and by what the last 19 s before the end looked like:
  KO       the chassis was destroyed (live: the robot kept moving, > 2 m from where the window began)
  IMMOB    the 20 s immobility rule (docs/29 12bg: the centre did not leave a sqrt 2 m sphere): in what pose (back = up.y < -0.5 /
           isUpsideDown for more than half of the window, tail = |up.y| < 0.3 for more than half, upright otherwise).
Live poses are the `.tsv` of the partsfight / firststroke captures (isUpsideDown, so tail and upright are not told apart: `notback`);
ours' are the `t<w>.tsv` of tools/oracle/ensemble.sh with `FIDTRACE_UP=1` (up.y in the last column). The match clock of the live
`.tsv` is the match's elapsed time (checked on `firststroke-...__004`, the Scout on its back 20 s before the end at 69 s).
The live match length is the first `O` row (the first robot out) as `live_ref.py` has it, capped at 120 s for the comparison with ours'
120 s fights; a fight with a tail-stat capture's brains' log (`AILOG=1`) is cut at the log's end (`tail_runs_cmp.match_end`).
Run from the repository root.
"""
import ast
import collections
import glob
import json
import math
import os
import re
import statistics as st
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "sideways"))
import live_ref as L  # noqa: E402
import tail_runs_cmp as T  # noqa: E402

ORIG = os.path.join(HERE, "..", "..", "local", "oracle", "original") + os.sep
CAP = 120.0
TAILSTAT = {"box": "boxstart-tailstat-scout-alarm-box", "octagon": "boxstart-tailstat-scout-alarm-octagon"}


def rows_of(path):
    out = []
    for line in open(path).read().split("\n")[1:]:
        if line.strip():
            t, x = line.split("\t")
            out.append((float(t), ast.literal_eval(x)))
    return out


def live_classes(pair):
    res = collections.Counter()
    lengths = []
    for r in L.live_rows(pair):
        if r["cls"] != "bell":
            continue
        base = ORIG + r["file"].replace("/", "__live__")
        lengths.append(r["end"] or 125.0)
        if not r["end"] or not os.path.exists(base + ".tsv"):
            continue
        w = [json.loads(x) for x in open(base + ".jsonl") if x.startswith('["W"')]
        if not w:
            continue
        out = 1 - w[0][3]
        win = [(t, x) for t, x in rows_of(base + ".tsv") if r["end"] - 19 <= t <= r["end"]]
        if len(win) < 50:
            continue
        pos = [x[out][1] for _, x in win]
        disp = max(math.dist(pos[0], p) for p in pos)
        back = sum(x[out][2] for _, x in win) / len(win)
        mode = "IMMOB" if disp < 2.0 else "KO"
        pose = "back" if back > 0.5 else "notback"
        res[("Scout" if out == 0 else "ALARM", mode, pose if mode == "IMMOB" else "-")] += 1
    for f in sorted(glob.glob(ORIG + TAILSTAT[pair] + "__live__*.jsonl")):
        e = T.match_end(f)
        if e:
            lengths.append(e)
    return res, lengths


def ours_classes(d):
    res = collections.Counter()
    lengths = []
    timeouts = 0
    for f in sorted(glob.glob(d + "/t*.tsv"), key=lambda s: int(re.findall(r"t(\d+)\.tsv", s)[0])):
        w = re.findall(r"t(\d+)\.tsv", f)[0]
        log = [x for x in open(d + "/log%s.txt" % w) if x.startswith("fight")]
        if not log:
            continue
        lengths.append(float(log[0].split()[1].rstrip("s,")))
        if "Timeout" in log[0]:
            timeouts += 1
            continue
        rows = rows_of(f)
        end = rows[-1][0]
        win = [(t, x) for t, x in rows if t >= end - 19]
        out = None
        for who in (0, 1):
            if rows[-1][1][who][4] <= 0:
                out = who
        disp = {}
        for who in (0, 1):
            pos = [x[who][1] for _, x in win]
            disp[who] = max(math.dist(pos[0], p) for p in pos)
        mode = "KO"
        if out is None:
            out = min((0, 1), key=lambda k: disp[k])
            mode = "IMMOB"
        up = [x[out][5] for _, x in win]
        back = sum(u < -0.5 for u in up) / len(up)
        tail = sum(abs(u) < 0.3 for u in up) / len(up)
        pose = "back" if back > 0.5 else "tail" if tail > 0.5 else "upright"
        res[("Scout" if out == 0 else "ALARM", mode, pose if mode == "IMMOB" else "-")] += 1
    return res, lengths, timeouts


def main():
    pair, d = sys.argv[1], sys.argv[2]
    lres, llen = live_classes(pair)
    ores, olen, tmo = ours_classes(d)
    print("live (bell-primed, W row + tsv): %d fights with a class, %d lengths" % (sum(lres.values()), len(llen)))
    for k, v in sorted(lres.items(), key=lambda kv: -kv[1]):
        print("  %3d  %s" % (v, k))
    print("ours: %d decided fights, %d timeouts" % (sum(ores.values()), tmo))
    for k, v in sorted(ores.items(), key=lambda kv: -kv[1]):
        print("  %3d  %s" % (v, k))
    cap = lambda v: [min(x, CAP) for x in v]  # noqa: E731
    for name, v in (("live", llen), ("ours", olen)):
        c = cap(v)
        print("%s: n %d, length (capped at 120 s) %.1f +- %.1f, under 40 s %.3f, under 60 s %.3f"
              % (name, len(v), st.mean(c), st.stdev(c) / len(c) ** 0.5, sum(x < 40 for x in v) / len(v), sum(x < 60 for x in v) / len(v)))
    for t in (40, 60):
        pl = sum(x < t for x in llen) / len(llen)
        po = sum(x < t for x in olen) / len(olen)
        se = math.sqrt(pl * (1 - pl) / len(llen) + po * (1 - po) / len(olen))
        print("under %d s: ours - live = %.3f, z = %.2f" % (t, po - pl, (po - pl) / se))


if __name__ == "__main__":
    main()
