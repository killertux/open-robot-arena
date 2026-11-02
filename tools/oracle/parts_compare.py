#!/usr/bin/env python3
"""parts_compare.py <ensemble dir> <victim> <attacker> <partsfight.jsonl>... [T]: the hits the
attacker lands on the victim's components in the first T seconds (default 20; 0 = the whole
fight), live against an ensemble (docs/29 section 12ar).  Robots are named as the K tables
of parts_live.py name them (Scout, ALARM, Sentinel, Wide Load); the ensemble's first bot is
robot 0.  Per component: fights with a hit of 10 hp or more, hits and hp per fight (mean).
Live: the H rows (0x417330, the priced hits) with the K map; ours: `FIDTRACE_HITS` rows
h<w>.jsonl."""
import collections
import glob
import json
import sys

sys.path.insert(0, __file__.rsplit("/", 1)[0])
import parts_live as pl


def live(paths, T, victim, attacker):
    per = []
    for f in paths:
        ev = pl.load(f)
        res = pl.analyse(f)
        off = res["off"]
        if off is None:
            continue
        robots = {}
        for t, r in ev:
            if r[0] == "K" and len(r) > 10 and r[3] == 0:
                robots[r[10]] = pl.ROBOTS.get(r[6], ("?", []))
        comp = {}
        for t, r in ev:
            if r[0] == "K" and len(r) > 10 and r[10] in robots:
                nm, ids = robots[r[10]]
                comp[r[4]] = (nm, ids[r[3]] if r[3] < len(ids) else str(r[3]))
        d = collections.defaultdict(lambda: [0, 0.0])
        for t, r in ev:
            # H: attPlayer, attComp, vicPlayer, vicComp, amount
            if r[0] == "H" and r[6] >= 10 and 0 <= t - off < (T or 1e9):
                v = comp.get(r[5])
                if v and v[0] == victim and comp.get(r[3], ("",))[0] == attacker:
                    d[v[1]][0] += 1
                    d[v[1]][1] += r[6]
        per.append(d)
    return per


def ours(dirn, T, vic):
    per = []
    for f in sorted(glob.glob(dirn + "/h*.jsonl")):
        d = collections.defaultdict(lambda: [0, 0.0])
        for l in open(f):
            h = json.loads(l)
            if h["amount"] >= 10 and h["t"] < (T or 1e9) and h["att"] == 1 - vic and h["vic"] == vic:
                d[h["comp"]][0] += 1
                d[h["comp"]][1] += h["amount"]
        per.append(d)
    return per


def main():
    T = float(sys.argv[-1]) if sys.argv[-1].replace(".", "").isdigit() else 20.0
    args = [a for a in sys.argv[1:] if not a.replace(".", "").isdigit()]
    victim, attacker = args[1], args[2]
    vic = 0 if victim in ("Scout", "Sentinel") else 1
    o, lv = ours(args[0], T, vic), live(args[3:], T, victim, attacker)
    names = sorted({k for d in o + lv for k in d})
    print(f"first {T:.0f} s, {attacker}'s hits of 10 hp or more on {victim}: live {len(lv)} fights, ours {len(o)}")
    for n in names:
        def stat(per):
            k = len(per)
            fights = sum(1 for d in per if n in d and d[n][0])
            return fights / k, sum(d[n][0] for d in per if n in d) / k, sum(d[n][1] for d in per if n in d) / k
        a, b = stat(lv), stat(o)
        print(f"  {n:16s} live: {a[0]*100:3.0f}% of fights, {a[1]:.2f} hits, {a[2]:6.1f} hp | ours: {b[0]*100:3.0f}%, {b[1]:.2f}, {b[2]:6.1f}")


if __name__ == "__main__":
    main()
