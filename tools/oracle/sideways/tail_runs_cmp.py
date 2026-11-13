"""tail_runs_cmp.py --live GLOB [GLOB ...] --ours DIR [--window 120] [--min 0.3]: the Scout's and ALARM's runs on their tails (|up.y| < 0.3)
in the live bell-primed fights (`S` rows of frida/ra2_robotpair.js; the bell is where the Scout first moves, docs/29 12bc; cut at the end of the match
when the capture has the brains' log, since the robots stay where they are after it) and in an ours
ensemble (`tools/oracle/ensemble.sh` directory with `FIDTRACE_UP=1`: t<w>.tsv), per fight: runs of at least --min s by duration class,
the time on the tail, the time both robots are on their tails (an A-frame), and a Poisson tail probability of ours' rate for the live
count of runs >= 3 s and >= 8 s (docs/29 12bg). Run from the repository root.
"""
import argparse
import ast
import glob
import json
import math


def match_end(path):
    """The game clock at which the match ended in a capture that carries the brains' log (`AILOG=1` of capture_boxstart.py): the last row of the
    log's last segment (the log of a session keeps every fight, each starting at clock 0); None without one."""
    try:
        meta = json.load(open(path.replace(".jsonl", ".json")))
        rows = ast.literal_eval(meta["ailog"])
    except (OSError, KeyError, ValueError, SyntaxError):
        return None
    last = [rows[0]]
    for r in rows[1:]:
        if r[0] < last[-1][0] - 1.0:
            last = []
        last.append(r)
    return last[-1][0]


def live_fights(patterns, window):
    fights = []
    for pat in patterns:
        for f in sorted(glob.glob(pat)):
            t = 0.0
            rows = []
            for line in open(f):
                if line.startswith('["A"'):
                    t += json.loads(line)[2]
                elif line.startswith('["S"'):
                    rows.append((t, json.loads(line)[2]))
            if not rows:
                continue
            first = rows[0][1]
            idx = min(range(len(first)), key=lambda i: (first[i][1] + 5.96) ** 2 + (first[i][3] - 5.75) ** 2)
            bell = None
            for tt, ch in rows:
                v = math.sqrt(sum(x * x for x in ch[idx][4:7]))
                if v > 0.8 and ch[idx][2] < 0.6:
                    bell = tt - 0.3
                    break
            if bell is None:
                continue
            end = match_end(f)
            w = window if end is None else min(window, end)
            fights.append([(tt - bell, abs(ch[idx][14]), abs(ch[1 - idx][14])) for tt, ch in rows if bell <= tt <= bell + w])
    return fights


def ours_fights(d, window):
    fights = []
    for f in sorted(glob.glob(d + "/t*.tsv")):
        rows = []
        for line in open(f).read().split("\n")[1:]:
            if line.strip():
                t, r = line.split("\t")
                a, b = ast.literal_eval(r)
                if float(t) <= window:
                    rows.append((float(t), abs(a[-1]), abs(b[-1])))
        if rows:
            fights.append(rows)
    return fights


def runs(rows, k, minlen):
    out, s = [], None
    for i, r in enumerate(rows):
        on = r[1 + k] < 0.3
        if on and s is None:
            s = r[0]
        if s is not None and (not on or i == len(rows) - 1):
            if r[0] - s >= minlen:
                out.append(r[0] - s)
            s = None
    return out


def poisson_tail(k, mu):
    """P(N <= k) for N ~ Poisson(mu)."""
    return sum(math.exp(-mu) * mu ** i / math.factorial(i) for i in range(k + 1))


def summary(name, fights, minlen):
    n = len(fights)
    res = {}
    for k, who in ((0, "Scout"), (1, "ALARM")):
        rs = [runs(f, k, minlen) for f in fights]
        allr = [x for r in rs for x in r]
        res[who] = allr
        cls = [sum(1 for x in allr if lo <= x < hi) for lo, hi in ((minlen, 1), (1, 3), (3, 8), (8, 1e9))]
        length = sum(f[-1][0] for f in fights)
        print("%-10s %s: %d fights (mean %.0f s), runs/fight %.2f, on the tail %.2f s/fight = %.1f %% of the fights; runs %.1f-1 / 1-3 / 3-8 / >=8 s: %d / %d / %d / %d (max %.1f s)" % (
            name, who, n, length / n, len(allr) / n, sum(allr) / n, 100 * sum(allr) / length, minlen, *cls, max(allr) if allr else 0))
    both = 0.0
    for f in fights:
        for i in range(1, len(f)):
            if f[i][1] < 0.3 and f[i][2] < 0.3:
                both += f[i][0] - f[i - 1][0]
    print("%-10s both on their tails: %.2f s/fight" % (name, both / n))
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--live", nargs="+", required=True)
    ap.add_argument("--ours", required=True)
    ap.add_argument("--window", type=float, default=120.0)
    ap.add_argument("--min", type=float, default=0.3)
    a = ap.parse_args()
    lf, of = live_fights(a.live, a.window), ours_fights(a.ours, a.window)
    L = summary("live", lf, a.min)
    O = summary("ours", of, a.min)
    for who in ("Scout", "ALARM"):
        for thr in (3.0, 8.0):
            lc = sum(1 for x in L[who] if x >= thr)
            rate = sum(1 for x in O[who] if x >= thr) / max(len(of), 1)
            print("%s runs >= %g s: live %d in %d fights; ours' rate %.3f/fight predicts %.1f, P(live count or fewer) = %.3f" % (
                who, thr, lc, len(lf), rate, rate * len(lf), poisson_tail(lc, rate * len(lf))))


if __name__ == "__main__":
    main()
