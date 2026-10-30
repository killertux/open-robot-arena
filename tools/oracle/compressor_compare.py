#!/usr/bin/env python3
"""Sentinel v Wide Load on the Compressor, live against ours (docs/29 section 12ai).

    compressor_compare.py opening <live boxstart-*.jsonl>... -- <ours.tsv>
    compressor_compare.py flips   <ensemble dir of t<w>.tsv> -- <live tsv>...

`opening`: the two robots' z against the match clock for the first seconds.
The live clock is the bell: the first `GP` row (the Wide Load's second brain
tick, `GetPath` at plus-time 0.2288 s in `aitrace`) minus 0.2288 s; the
`X` rows are the chassis' centre of mass every eighth step piece
(`frida/ra2_boxfight.js`, `tools/oracle/capture_boxstart.py`). Ours is
`FIDTRACE_COM=1 RA2_TRACE_EVERY=1 fidtrace match ...` for the same pair,
FIDTRACE_WARMUP unset (the robots fall at the bell).

`flips`: from the pose traces (live: `ra2_python.py --sample` tsv of
`capture_boxstart.py` / `capture_fightpos_set.sh`; ours: the ensemble's
`t<w>.tsv` with `FIDTRACE_UP=1 FIDTRACE_COM=1`, the last column the chassis'
up.y), per robot: flips (up.y < 0 / isUpsideDown) by 40 s, permanent flips by
25 s, falls off the ramps' sides and how many end on the back, the share of
fights decided by 40 / 60 / 100 s (live: the last time a brain had a tactic),
and the Wide Load's stalls on its start plateau (x < -11 for more than 5 s).
"""
import glob
import json
import math
import re
import statistics as st
import sys

PAT_LIVE = re.compile(r"\((\d), \(([-\d.e]+), ([-\d.e]+), ([-\d.e]+)\), (\d), '([\w-]+)'")
PAT_OURS = re.compile(
    r"\((\d), \(([-\d.e]+), ([-\d.e]+), ([-\d.e]+)\), ([-\d.e]+), ([-\d.e]+), ([-\d.e]+), ([-\d.e]+)\)"
)


def live_x(path):
    """(bell-based t, wl_centre, sentinel_centre) per `X` row of a boxstart jsonl."""
    t, first_gp, rows = 0.0, None, []
    for line in open(path):
        r = json.loads(line)
        if r[0] == "A":
            t += r[2]
        elif r[0] == "GP" and first_gp is None:
            first_gp = t
        elif r[0] == "X":
            rows.append((t, r[2]))
    bell = first_gp - 0.2288
    start = {q[0]: q[1] for q in rows[0][1]}
    wl = next(k for k in start if start[k][0] < 0)
    se = next(k for k in start if start[k][0] > 0)
    out = []
    for tt, bodies in rows:
        d = {q[0]: q[1] for q in bodies}
        out.append((tt - bell, d[wl], d[se]))
    return out


def ours_com(path):
    out = []
    for line in open(path).readlines()[1:]:
        t, rest = line.split("\t")
        d = {int(m[0]): (float(m[1]), float(m[2]), float(m[3])) for m in PAT_OURS.findall(rest)}
        out.append((float(t), d[1], d[0]))
    return out


def lerp(tr, t, k):
    for a, b in zip(tr, tr[1:]):
        if a[0] <= t <= b[0]:
            u = (t - a[0]) / (b[0] - a[0]) if b[0] > a[0] else 0.0
            return [a[k][j] + (b[k][j] - a[k][j]) * u for j in range(3)]
    return None


def opening(lives, ours):
    runs = [live_x(p) for p in lives]
    o = ours_com(ours)
    print("t     Wide Load z: live mean (sd) | ours     Sentinel z: live mean (sd) | ours")
    for T in (0.4, 0.6, 0.8, 1.0, 1.2, 1.4, 1.6, 2.0):
        row = []
        for k in (1, 2):
            v = [lerp(r, T, k)[2] for r in runs]
            row.append("%7.2f (%.2f) | %7.2f" % (st.mean(v), st.pstdev(v), lerp(o, T, k)[2]))
        print("%.1f  %s     %s" % (T, row[0], row[1]))


def read_live(path):
    rows = []
    for line in open(path).readlines()[1:]:
        t, rest = line.split("\t")
        d = {
            int(m.group(1)): (float(m.group(2)), float(m.group(3)), float(m.group(4)), int(m.group(5)))
            for m in PAT_LIVE.finditer(rest)
        }
        if len(d) == 2:
            rows.append((float(t), d))
    last_active = None
    for line in open(path).readlines()[1:]:
        t, rest = line.split("\t")
        tac = [m.group(6) for m in PAT_LIVE.finditer(rest)]
        if len(tac) == 2 and (tac[0] != "-" or tac[1] != "-"):
            last_active = float(t)
    return rows, last_active


def read_ours(d):
    runs = []
    for f in sorted(glob.glob(d + "/t*.tsv"), key=lambda s: int(re.findall(r"t(\d+)\.tsv", s)[0])):
        rows = []
        for line in open(f).readlines()[1:]:
            t, rest = line.split("\t")
            ms = PAT_OURS.findall(rest)
            dd = {int(m[0]): (float(m[1]), float(m[2]), float(m[3]), 1 if float(m[7]) < 0 else 0) for m in ms}
            rows.append((float(t), dd))
        log = re.sub(r"t(\d+)\.tsv$", r"log\1.txt", f)
        end = None
        try:
            for line in open(log):
                m = re.match(r"fight ([\d.]+)s, reason Some\((\w+)\)", line)
                if m and m.group(2) == "LastRobot":
                    end = float(m.group(1))
        except OSError:
            pass
        runs.append((rows, end))
    return runs


def first_flip(r, p, tmax=60.0):
    return next((t for t, d in r if t <= tmax and d[p][3]), None)


def permanent(r, p, tf=25.0, hold=15.0):
    for t, d in r:
        if t <= tf and d[p][3]:
            tail = [d2[p][3] for t2, d2 in r if t <= t2 <= t + hold]
            if tail and all(tail):
                return t
    return None


def falls(r, p, tmax=40.0):
    out, last = [], -9.0
    for i, (t, d) in enumerate(r):
        if t > tmax:
            break
        if d[p][1] > 1.9 and abs(d[p][0]) > 8:
            for t2, d2 in r[i + 1 : i + 11]:
                if t2 - t > 1.0:
                    break
                if d2[p][1] < 1.2 and abs(d2[p][2]) > abs(d[p][2]) + 0.5 and t - last > 2:
                    out.append(any(d3[p][3] for t3, d3 in r if t2 <= t3 <= t2 + 2.0))
                    last = t
                    break
    return out


def stall(r):
    t_in = [t for t, d in r if 3 < t < 30 and d[1][0] < -11.0]
    return max(t_in) - min(t_in) if t_in else 0.0


def fisher(a, n1, b, n2):
    """two-sided Fisher exact p of a/n1 against b/n2."""
    from math import comb

    k = a + b
    tot = comb(n1 + n2, k)
    p0 = comb(n1, a) * comb(n2, b) / tot
    p = 0.0
    for x in range(max(0, k - n2), min(n1, k) + 1):
        px = comb(n1, x) * comb(n2, k - x) / tot
        if px <= p0 * (1 + 1e-9):
            p += px
    return p


def summary(name, runs):
    n = len(runs)
    rows = [r for r, _ in runs]
    ends = [e for _, e in runs]
    c = {}
    c["S flips by 40 s"] = sum(first_flip(r, 0, 40) is not None for r in rows)
    c["WL flips by 40 s"] = sum(first_flip(r, 1, 40) is not None for r in rows)
    c["permanent flip by 25 s (either)"] = sum(
        permanent(r, 0) is not None or permanent(r, 1) is not None for r in rows
    )
    for p, nm in ((0, "S"), (1, "WL")):
        f = [e for r in rows for e in falls(r, p)]
        c["%s falls off a ramp side (per run %.2f), on its back after" % (nm, len(f) / n)] = (len(f), sum(f))
    for T in (40, 60, 100):
        c["decided by %d s" % T] = sum(e is not None and e <= T for e in ends)
    c["WL stalled on its plateau > 5 s"] = sum(stall(r) > 5 for r in rows)
    print("%s (n=%d)" % (name, n))
    for k, v in c.items():
        print("  %-62s %s" % (k, v))
    return c


def flips(ours_dir, lives):
    lv = []
    for p in lives:
        rows, last = read_live(p)
        lv.append((rows, last))
    ov = read_ours(ours_dir)
    # ours: `end` None means the fight went the distance
    a = summary("live", lv)
    b = summary("ours", ov)
    for k in ("S flips by 40 s", "WL flips by 40 s", "permanent flip by 25 s (either)", "decided by 60 s",
              "decided by 100 s", "WL stalled on its plateau > 5 s"):
        print("  Fisher p %-40s %.3f" % (k, fisher(a[k], len(lv), b[k], len(ov))))


def main():
    a = sys.argv[1:]
    if not a or "--" not in a:
        print(__doc__)
        sys.exit(1)
    k = a.index("--")
    head, tail = a[1:k], a[k + 1 :]
    if a[0] == "opening":
        opening(head, tail[0])
    elif a[0] == "flips":
        flips(head[0], tail)
    else:
        print(__doc__)


if __name__ == "__main__":
    main()
