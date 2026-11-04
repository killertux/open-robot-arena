#!/usr/bin/env python3
"""strike_points.py <live firststroke jsonl glob> <ours dir of first_stroke_ours.sh> [min hp]:
where ALARM's hammer lands on the Scout (docs/29 section 12at): the priced hits' points in the
Scout's chassis frame, per victim component, live (`H` rows with the chassis' pose from the `B` rows
of frida/ra2_firststroke.js) against ours (`h<w>.jsonl` with `b<w>.txt`: the body position is its centre of mass), first 10 s of the fight.
Live poses are a centre of mass and three rows; the chassis' centre of mass is (0, 0.2154, -0.0635)
in its frame (bodies-scout-alarm__live__000)."""
import collections
import glob
import json
import os
import statistics as st
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import first_stroke as fs
import parts_live as pl

COM = (0.0, 0.2154, -0.0635)


def live(paths, lo):
    out = []
    for p in paths:
        rows = [json.loads(l) for l in open(p)]
        hits, series, ids = fs.live_fight(p)
        if ids[0] is None or not series:
            continue
        robots, comp = {}, {}
        for r in rows:
            if r[0] == "K" and len(r) > 10 and r[3] == 0:
                robots[r[10]] = pl.ROBOTS.get(r[6], ("?", []))
        for r in rows:
            if r[0] == "K" and len(r) > 10 and r[10] in robots:
                nm, names = robots[r[10]]
                comp[r[4]] = (nm, names[r[3]] if r[3] < len(names) else str(r[3]))
        off = pl.analyse(p)["off"]
        t = 0.0
        for r in rows:
            if r[0] == "A":
                t += r[2]
            elif r[0] == "H" and r[2] == 1 and r[4] == 0 and r[3] in comp and r[5] in comp and r[6] >= lo and 0 < t - off < 10:
                ts = min(series, key=lambda s: abs(s[0] - (t - off)))[1]
                sc = ts[ids[0]]
                M = [sc[4:7], sc[7:10], sc[10:13]]
                d = [r[7][i] - sc[1 + i] for i in range(3)]
                loc = [sum(M[i][j] * d[j] for j in range(3)) + COM[i] for i in range(3)]
                out.append((comp[r[5]][1], loc, r[6], comp[r[3]][1]))
    return out


def ours(d, lo):
    out = []
    for h in sorted(glob.glob(d + "/h*.jsonl")):
        w = os.path.basename(h)[1:].split(".")[0]
        b = {}
        for l in open(f"{d}/b{w}.txt"):
            a = l.split()
            if int(a[2]) == 0 and int(a[3]) == 0:
                b[round(float(a[1]), 4)] = list(map(float, a[4:]))
        for l in open(h):
            r = json.loads(l)
            if r["att"] != 1 or r["vic"] != 0 or r["amount"] < lo or r["t"] > 10:
                continue
            k = min(b, key=lambda t: abs(t - r["t"]))
            v = b[k]
            M = fs.rows_rot(v[3:7])
            dd = [r["p"][i] - v[i] for i in range(3)]
            out.append((r["comp"], [sum(M[i][j] * dd[j] for j in range(3)) + COM[i] for i in range(3)], r["amount"], r["by"]))
    return out


def show(name, rows, nfights):
    print(f"== {name}: {len(rows)} hits of ALARM on the Scout in the first 10 s, {nfights} fights")
    by = collections.defaultdict(list)
    for c, loc, a, by_c in rows:
        by[c].append((loc, a, by_c))
    for c, v in sorted(by.items(), key=lambda kv: -len(kv[1])):
        z = [x[0][2] for x in v]
        y = [x[0][1] for x in v]
        print(f"   {c:18s} n {len(v):3d} hp/hit {st.mean(x[1] for x in v):5.1f}  forward z {st.median(z):5.2f} ({min(z):5.2f}..{max(z):5.2f})  height y {st.median(y):5.2f} ({min(y):5.2f}..{max(y):5.2f})")


if __name__ == "__main__":
    lo = float(sys.argv[3]) if len(sys.argv) > 3 else 10.0
    paths = sorted(glob.glob(sys.argv[1]))
    L = live(paths, lo)
    show("live", L, len(paths))
    O = ours(sys.argv[2], lo)
    show("ours", O, len(glob.glob(sys.argv[2] + "/h*.jsonl")))
