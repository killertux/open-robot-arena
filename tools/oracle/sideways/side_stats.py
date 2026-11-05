#!/usr/bin/env python3
"""side_stats.py <ensemble dir with t*.tsv (FIDTRACE_UP=1)> <live robotpair/sidefight jsonl>...: how long the Scout (player 0) stands on its
side/tail (|up.y| < 0.25 for the share, < 0.3 for the runs) in the live fights from the bell and in an ours ensemble (docs/29 12bd).
Live: the `S` rows of ra2_robotpair.js (both chassis, each resolver entry; the Scout is the body nearest the Octagon's first start point,
the other the second robot); the bell is where the Scout first moves (speed > 0.8 m/s, 0.3 s before). Prints the share of time and the
number of fights with a run of >= 2, 4, 8 s within the first 120 s."""
import ast
import glob
import json
import math
import statistics as st
import sys


def live_series(path):
    t, S = 0.0, []
    for line in open(path):
        if line.startswith('["A"'):
            t += json.loads(line)[2]
        elif line.startswith('["S"'):
            r = json.loads(line)
            S.append((t, r[2]))
    first = S[0][1]
    idx = min(range(len(first)), key=lambda i: (first[i][1] + 5.96) ** 2 + (first[i][3] - 5.75) ** 2)
    bell = None
    for t, ch in S:
        v = math.sqrt(sum(x * x for x in ch[idx][4:7]))
        if v > 0.8 and ch[idx][2] < 0.6 and bell is None:
            bell = t - 0.3
    # flat row: ptr, com(3), vel(3), ang(3), rot(9); rot[4] is the chassis' up axis' height
    return [(t - bell, ch[idx][14]) for t, ch in S if t >= bell]


def runs(ts, ys, thr=0.3):
    out, s0 = [], None
    for t, y in zip(ts, ys):
        s = abs(y) < thr
        if s and s0 is None:
            s0 = t
        if not s and s0 is not None:
            out.append(t - s0)
            s0 = None
    if s0 is not None:
        out.append(ts[-1] - s0)
    return out


def summarize(name, fights):
    share = [sum(abs(y) < 0.25 for y in ys) / len(ys) for ts, ys in fights]
    f = lambda k: sum(any(r >= k for r in runs(ts, ys)) for ts, ys in fights)
    n = len(fights)
    print("%-6s n=%3d  side share %.1f +- %.1f %%  fights with a run >= 2 s: %d (%.0f %%), >= 4 s: %d (%.0f %%), >= 8 s: %d (%.0f %%)" % (
        name, n, 100 * st.mean(share), 100 * st.stdev(share) / math.sqrt(n), f(2), 100 * f(2) / n, f(4), 100 * f(4) / n, f(8), 100 * f(8) / n))


if __name__ == "__main__":
    ours = []
    for f in glob.glob(sys.argv[1] + "/t*.tsv"):
        ts, ys = [], []
        for line in open(f).read().split("\n")[1:]:
            if line.strip():
                t, r = line.split("\t")
                if float(t) > 120:
                    break
                a, _ = ast.literal_eval(r)
                ts.append(float(t))
                ys.append(a[-1])
        ours.append((ts, ys))
    live = []
    for f in sys.argv[2:]:
        s = [x for x in live_series(f) if x[0] <= 120]
        live.append(([x[0] for x in s], [x[1] for x in s]))
    summarize("ours", ours)
    if live:
        summarize("live", live)
