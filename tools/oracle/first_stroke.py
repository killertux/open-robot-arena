#!/usr/bin/env python3
"""first_stroke.py window|order|pose|stroke <live jsonl glob> <ours dir> (docs/29 section 12as).

The first blow of ALARM's hammer on the Scout, live against ours, with one definition:
  live  the `firststroke-scout-alarm-box__live__*.jsonl` captures (frida/ra2_firststroke.js:
        `B` rows = every Havok body of both robots every step piece; `H` the priced hits);
  ours  a directory made by first_stroke_ours.sh (b<w>.txt every body per step, h<w>.jsonl hits).
  order   the components the hammer head prices (any size) in the first 0.1 s after its first hit;
  window  the Scout components ALARM's hammer hits (>= 10 hp) in the first W s (default 0.4)
          after the first such hit: share of fights and hp per fight;
  pose    at the step before that hit: the hammer head's centre in the Scout's frame, the
          Scout's fork (extender + arm) in its chassis frame, ALARM's pitch against the Scout,
          the hammer rod's travel in ALARM's frame (median, mean, range);
  stroke  when the rod passes 1.0 m after the first hit, and its deepest push-in in 0.3 s.
Body rotations: live `+0xcc` is three rows, the body's axes in the world (local = M world).
"""
import collections
import glob
import json
import math
import os
import statistics as st
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import parts_live as pl


def mv(m, v):
    return [sum(m[i][j] * v[j] for j in range(3)) for i in range(3)]


def rows_rot(q):  # quaternion (x y z w) -> local-from-world rows
    x, y, z, w = q
    R = [[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
         [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
         [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]]
    return [[R[j][i] for j in range(3)] for i in range(3)]


def live_fight(path):
    """-> (hits [(clock, attacker comp, victim comp, amount)], series [(clock, bodies)], ids)"""
    rows = [json.loads(l) for l in open(path)]
    robots, comp, mass = {}, {}, {}
    for r in rows:
        if r[0] == "K" and len(r) > 10 and r[3] == 0:
            robots[r[10]] = pl.ROBOTS.get(r[6], ("?", []))
        elif r[0] == "BP":
            mass[r[2]] = r[4][0x190 // 4]
    for r in rows:
        if r[0] == "K" and len(r) > 10 and r[10] in robots:
            nm, ids = robots[r[10]]
            comp[r[4]] = (nm, ids[r[3]] if r[3] < len(ids) else str(r[3]))
    sc = next((i for i, m in mass.items() if abs(m - 163.49) < 1), None)
    al = next((i for i, m in mass.items() if abs(m - 232.1) < 1), None)
    t, off, hits, series = 0.0, pl.analyse(path)["off"], [], []
    for r in rows:
        if r[0] == "A":
            t += r[2]
        elif r[0] == "H" and r[2] == 1 and r[4] == 0 and r[3] in comp and r[5] in comp and off is not None:
            hits.append((t - off, comp[r[3]][1], comp[r[5]][1], r[6]))
        elif r[0] == "B" and off is not None:
            series.append((t - off, {x[0]: x for x in r[2]}))
    return hits, series, (sc, al)


def live_pose(s, ids):
    sc, al = s[ids[0]], s[ids[1]]
    ms, ma = [sc[4:7], sc[7:10], sc[10:13]], [al[4:7], al[7:10], al[10:13]]
    rod, fork = s[7], s[0]
    return sc, al, ms, ma, rod, fork


def feats(sc, al, ms, ma, rod, fork):
    head = mv(ms, [rod[1 + i] - sc[1 + i] for i in range(3)])
    rel = mv(ma, [rod[1 + i] - al[1 + i] for i in range(3)])
    fk = mv(ms, [fork[1 + i] - sc[1 + i] for i in range(3)])
    fwd = ma[2]
    pitch = math.degrees(math.asin(max(-1, min(1, mv(ms, fwd)[1]))))
    return dict(hx=head[0], hy=head[1], hz=head[2], pitch=pitch, fy=fk[1], fz=fk[2], rod=rel[2])


def ours_files(d):
    for h in sorted(glob.glob(d + "/h*.jsonl"), key=lambda p: int(os.path.basename(p)[1:].split(".")[0])):
        w = os.path.basename(h)[1:].split(".")[0]
        hits = []
        for l in open(h):
            r = json.loads(l)
            if r["att"] == 1 and r["vic"] == 0:
                hits.append((r["t"], r["by"], r["comp"], r["amount"]))
        b = {}
        if os.path.exists(f"{d}/b{w}.txt"):  # `window` needs only the hits (an ensemble.sh directory)
            for l in open(f"{d}/b{w}.txt"):
                a = l.split()
                b.setdefault(round(float(a[1]), 4), {})[(int(a[2]), int(a[3]))] = list(map(float, a[4:]))
        yield hits, b


def ours_state(s):
    def conv(v):
        m = rows_rot(v[3:7])
        return [0, *v[0:3], *m[0], *m[1], *m[2], *v[7:10], *v[10:13]]
    c0, c1, rod, fork = conv(s[(0, 0)]), conv(s[(1, 0)]), conv(s[(1, 5)]), conv(s[(0, 3)])
    ms, ma = [c0[4:7], c0[7:10], c0[10:13]], [c1[4:7], c1[7:10], c1[10:13]]
    return c0, c1, ms, ma, rod, fork


def first_big(hits):
    return next((h for h in hits if h[3] >= 10 and h[0] > 0), None)


def window(live, ours, W):
    def tab(name, fights):
        comps = collections.defaultdict(lambda: [0, 0.0])
        n = 0
        for hits in fights:
            f = first_big(hits)
            if not f:
                continue
            n += 1
            seen = collections.defaultdict(float)
            for t, a, v, x in hits:
                if f[0] <= t <= f[0] + W and x >= 10:
                    seen[v] += x
            for v, x in seen.items():
                comps[v][0] += 1
                comps[v][1] += x
        print(f"{name}: {n} fights, first {W} s after the first hit of 10 hp or more")
        for v, (k, x) in sorted(comps.items(), key=lambda kv: -kv[1][0]):
            print(f"   {v:18s} fights {k}/{n} ({100 * k / n:3.0f} %), hp per fight {x / n:5.0f}")
    tab("live", [live_fight(p)[0] for p in live])
    tab("ours", [h for h, _ in ours_files(ours)])


def order(live, ours):
    """The Scout components the hammer head prices (any size) in the first 0.1 s after its first hit."""
    def tab(name, fights):
        first, seq = collections.Counter(), collections.Counter()
        for hits in fights:
            ev = [(t, v) for t, a, v, x in hits if a == "Hammer Head" and t > 0]
            if not ev:
                continue
            first[ev[0][1]] += 1
            seq[tuple(v for t, v in ev if t <= ev[0][0] + 0.1)] += 1
        print(f"{name}: first priced component {dict(first)}")
        for k, n in seq.most_common(6):
            print(f"   {n:3d} {k}")
    tab("live", [live_fight(p)[0] for p in live])
    tab("ours", [h for h, _ in ours_files(ours)])


def pose(live, ours):
    def show(name, recs):
        print(f"{name}: {len(recs)} fights")
        for k in ("hx", "hy", "hz", "pitch", "fy", "fz", "rod"):
            v = [r[k] for r in recs]
            print(f"   {k:6s} median {st.median(v):7.3f} mean {st.mean(v):7.3f} range {min(v):7.3f}..{max(v):7.3f}")
    L = []
    for p in live:
        hits, series, ids = live_fight(p)
        f = first_big(hits)
        if not f or ids[0] is None:
            continue
        prev = [s for t, s in series if t <= f[0]]
        if prev:
            L.append(feats(*live_pose(prev[-1], ids)))
    show("live (the step before the first hit)", L)
    O = []
    for hits, b in ours_files(ours):
        f = first_big(hits)
        if not f:
            continue
        ts = sorted(b)
        i = min(range(len(ts)), key=lambda i: abs(ts[i] - f[0]))
        O.append(feats(*ours_state(b[ts[max(0, i - 1)]])))
    show("ours", O)


def stroke(live, ours):
    def after(series_fn, t0):
        s = [(t - t0, z) for t, z in series_fn if -0.1 < t - t0 < 1.0]
        t1 = next((a for a, z in s if z > 1.0), None)
        return t1, min(z for a, z in s if a < 0.3)
    L, O = [], []
    for p in live:
        hits, series, ids = live_fight(p)
        f = first_big(hits)
        if not f or ids[0] is None:
            continue
        ser = []
        for t, s in series:
            sc, al, ms, ma, rod, fork = live_pose(s, ids)
            ser.append((t, mv(ma, [rod[1 + i] - al[1 + i] for i in range(3)])[2]))
        if any(-0.1 < t - f[0] < 0.3 for t, _ in ser):
            L.append(after(ser, f[0]))
    for hits, b in ours_files(ours):
        f = first_big(hits)
        if not f:
            continue
        ser = []
        for t in sorted(b):
            if not all(k in b[t] for k in ((0, 0), (1, 0), (1, 5), (0, 3))):
                continue
            c0, c1, ms, ma, rod, fork = ours_state(b[t])
            ser.append((t, mv(ma, [rod[1 + i] - c1[1 + i] for i in range(3)])[2]))
        O.append(after(ser, f[0]))
    for name, R in (("live", L), ("ours", O)):
        t = [r[0] for r in R if r[0] is not None]
        print(f"{name}: {len(R)} fights; rod past 1.0 m {st.median(t):.3f} s after the first hit "
              f"(range {min(t):.2f}..{max(t):.2f}), deepest push-in median {st.median(r[1] for r in R):.2f} m")


def main():
    if len(sys.argv) < 4:
        print(__doc__)
        return
    mode, live, ours = sys.argv[1], sorted(glob.glob(sys.argv[2])), sys.argv[3]
    if mode == "window":
        window(live, ours, float(sys.argv[4]) if len(sys.argv) > 4 else 0.4)
    elif mode == "order":
        order(live, ours)
    elif mode == "pose":
        pose(live, ours)
    elif mode == "stroke":
        stroke(live, ours)


if __name__ == "__main__":
    main()
