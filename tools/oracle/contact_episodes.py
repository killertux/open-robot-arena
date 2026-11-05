#!/usr/bin/env python3
"""contact_episodes.py <label> <live fightpos glob> <ensemble dir> [warmups]  (docs/29 section 12aw)

Contact occupancy and episodes of two robots, live against an ensemble.

Live: `fightpos-*__live__*.tsv` (capture_fightpos_set.sh: `ra2_python.py --sample` every 6 frames, 0.1 s:
location, isUpsideDown, last tactic, commanded throttle and turn, the sensor table).  Ours: the
`log<w>.txt` of an `ensemble.sh` run with `RA2_AI_TRACE=1` (one `ai t=...` line per brain tick, the
same fields), the robots' planar centres (`getLocation`).

Window: from the first sample in which both robots are in Engage and closer than APPR (8 m) for 60 s, only
the samples with both upright.  Reported: the share of samples with the two centres closer than TH (3 m),
the share with the sensor table's chassis flag of either robot, and the episodes (runs of centre
distance < TH, gaps shorter than 0.25 s merged): per minute, mean/median/p90 length, the separation
speed over the 0.6 s after the end and the approach speed over the 0.5 s before the start (m/s, positive
apart), the median minimum distance.  Usage: contact_episodes.py comp 'local/oracle/original/fightpos-sentinel-wideload-compressor__live__0*.tsv' DIR/comp 96
"""
import ast
import glob
import math
import re
import statistics as st
import sys

APPR = 8.0
TH = 3.0
GAP = 0.25
PAT = re.compile(r"ai t=([\d.]+) bot=(\d) \(([-\d.]+),([-\d.]+)\) h=\S+ v=\S+ w=\S+ tactic=(\S+) throttle=(\S+) turn=(\S+) route=.* sensors=(\{.*?\}) events")


def live_series(path):
    out = []
    for line in open(path).read().split("\n")[1:]:
        if not line.strip():
            continue
        t, r = line.split("\t")
        r = ast.literal_eval(r)
        if len(r) < 2:
            continue
        a, b = r[0], r[1]
        out.append(dict(t=float(t), p=[(a[1][0], a[1][2]), (b[1][0], b[1][2])], up=[not a[2], not b[2]],
                        tac=[a[3], b[3]], ch=[any(x[2] for x in a[6]), any(x[2] for x in b[6])]))
    return out


def ours_series(d, w):
    last, out = {}, []
    for line in open(f"{d}/log{w}.txt"):
        m = PAT.match(line)
        if not m:
            continue
        b = int(m[2])
        s = eval(m[8])
        last[b] = dict(t=float(m[1]), p=(float(m[3]), float(m[4])), tac=m[5], ch=any(v[1] for v in s.values()))
        if b == 1 and len(last) == 2:
            a, c = last[0], last[1]
            out.append(dict(t=a["t"], p=[a["p"], c["p"]], up=[a["tac"] != "Invert", c["tac"] != "Invert"],
                            tac=[a["tac"], c["tac"]], ch=[a["ch"], c["ch"]]))
    return out


def sep(r):
    return math.dist(r["p"][0], r["p"][1])


def window(s, secs=60.0):
    i0 = next((i for i, r in enumerate(s) if r["tac"] == ["Engage", "Engage"] and sep(r) < APPR), None)
    if i0 is None:
        return []
    return [r for r in s[i0:] if r["t"] - s[i0]["t"] <= secs]


def episodes(s):
    runs, cur = [], None
    for i, r in enumerate(s):
        inside = r["up"][0] and r["up"][1] and sep(r) < TH
        if inside:
            if cur is None:
                cur = [i, i]
            else:
                cur[1] = i
        elif cur is not None and (not (r["up"][0] and r["up"][1]) or r["t"] - s[cur[1]]["t"] > GAP):
            runs.append(cur)
            cur = None
    if cur:
        runs.append(cur)
    out = []
    for i0, i1 in runs:
        t0, t1 = s[i0]["t"], s[i1]["t"]
        j = next((k for k in range(i1, len(s)) if s[k]["t"] - t1 >= 0.6), None)
        k0 = next((k for k in range(i0, -1, -1) if t0 - s[k]["t"] >= 0.5), None)
        if j is None or k0 is None:
            continue
        out.append(dict(len=t1 - t0 + 0.1, out=(sep(s[j]) - sep(s[i1])) / (s[j]["t"] - t1),
                        inn=(sep(s[i0]) - sep(s[k0])) / (t0 - s[k0]["t"]), minsep=min(sep(s[k]) for k in range(i0, i1 + 1))))
    return out


def stats(fights):
    n = c = chs = chw = 0
    eps = []
    for s in fights:
        w = window(s)
        ok = [r for r in w if r["up"][0] and r["up"][1]]
        n += len(ok)
        c += sum(sep(r) < TH for r in ok)
        chs += sum(r["ch"][0] for r in ok)
        chw += sum(r["ch"][1] for r in ok)
        eps += episodes(w)
    q = lambda xs, p: sorted(xs)[int(p * (len(xs) - 1))]
    ln = [e["len"] for e in eps]
    return dict(samples=n, occ=c / n, chS=chs / n, chW=chw / n, eps_min=len(eps) / (n * 0.1 / 60), len_mean=st.mean(ln),
                len_med=q(ln, .5), len_p90=q(ln, .9), out_mean=st.mean(e["out"] for e in eps), in_mean=st.mean(e["inn"] for e in eps),
                minsep_med=q([e["minsep"] for e in eps], .5))


def main():
    label, pat, d = sys.argv[1:4]
    nw = int(sys.argv[4]) if len(sys.argv) > 4 else 96
    live = [live_series(f) for f in sorted(glob.glob(pat))]
    ours = [ours_series(d, w) for w in range(nw)]
    for name, fs in (("live", live), ("ours", ours)):
        print(label, name, len(fs), " ".join(f"{k} {v:.2f}" for k, v in stats(fs).items()))


if __name__ == "__main__":
    main()
