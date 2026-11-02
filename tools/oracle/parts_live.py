#!/usr/bin/env python3
"""parts_live.py <partsfight-*.jsonl>...: component damage and break-off of the
live fights captured with frida/ra2_parts.js (docs/29 section 12ar).

Per fight: the length (the match clock), who is out and why, and per robot the
components struck (blows of 10 hp or more, the sub-10 splash, hp lost, hp left),
the detach queue (0x417d70) with the blow that triggered it and the components
that came away with it. `--summary` prints one line per fight and the totals.
"""
import collections
import json
import statistics
import sys

# Robots by chassis hitpoints (the .bot's chassis); id -> name of each component.
ROBOTS = {
    1828: ("Scout", ["Chassis", "Burst Motor", "Angle Motor L", "Mini wheel L", "Angle Motor R",
                     "Mini wheel R", "Battery", "Control Board", "Round Extender",
                     "Forklift Arm", "SmartZone"]),
    2662: ("ALARM", ["Chassis", "Angle Motor", "Mini wheel", "Angle Motor", "Mini wheel",
                     "Angle Motor", "Mini wheel", "Angle Motor", "Mini wheel", "Battery",
                     "CO2 Tank", "CO2 Tank", "Burst Piston", "Hammer Head", "Control Board",
                     "SmartZone"]),
    3156: ("Sentinel", ["Chassis", "Spin Motor R", "Spin Motor L", "Wheel L rear", "Wheel R rear",
                        "Battery", "Control Board", "Steering Unit", "Wheel R front",
                        "Wheel L front", "Burst Motor", "Bracket Wedge", "SmartZone"]),
    2144: ("Wide Load", ["Chassis", "Z-Tek R", "Z-Tek L", "Burst Piston", "N-12 L", "N-12 R",
                         "Axle Mount L", "Axle Mount R", "Mini wheel R", "Mini wheel L", "Battery",
                         "Air Tank", "Control Board", "T Connector", "T Connector L",
                         "T Connector R", "Angle Connector R", "Angle Connector L", "Tip R",
                         "Tip M R", "Tip M L", "Tip L", "SmartZone"]),
    2319: ("Razor", ["Chassis", "Angle Motor", "Mini wheel", "Angle Motor", "Mini wheel",
                     "Angle Motor", "Angle Motor", "Mini wheel", "Mini wheel", "Battery",
                     "Baseplate Anchor", "Spike Strip", "Control Board", "Baseplate Anchor",
                     "Spike Strip"]),
    1820: ("FlapJack", ["Chassis", "Burst Motor", "Angle Motor", "Mini wheel", "Angle Motor",
                        "Mini wheel", "Battery", "Balance Caster", "Square Extender",
                        "Control Board", "Small Wedge", "SmartZone", "Battery Pack",
                        "Battery Pack"]),
}


def load(path):
    rows = [json.loads(l) for l in open(path)]
    t = 0.0
    out = []
    for r in rows:
        if r[0] == "A":
            t += r[2]
        out.append((t, r))
    return out


def analyse(path):
    ev = load(path)
    # the clock offset: A-sum time minus the match's elapsed seconds
    def is_clock(r):
        return r[0] == "MC" or (r[0] == "T" and len(r) == 6)  # 'T' before the rename

    offs = [t - r[2] for t, r in ev if is_clock(r) and r[4] == 1 and r[2] > 1.0]
    off = statistics.median(offs) if offs else None
    robots = {}  # robot ptr -> [name, ids]
    comp = {}  # comp ptr -> (robot name, id, maxhp, fracture)
    chassis_of = {}
    for t, r in ev:
        if r[0] == "K" and len(r) > 10:
            if r[3] == 0:
                nm = ROBOTS.get(r[6], ("?%d" % r[6], []))[0]
                robots[r[10]] = nm
    for t, r in ev:
        if r[0] == "K" and len(r) > 10 and r[10] in robots:
            comp[r[4]] = (robots[r[10]], r[3], r[6], r[7])
    names = {n: ids for n, ids in ROBOTS.values()}

    def label(p):
        c = comp.get(p)
        if not c:
            return p
        nm = names.get(c[0], [])
        return "%s#%d %s" % (c[0], c[1], nm[c[1]] if c[1] < len(nm) else "")

    def clock(t):
        return t - off if off is not None else t

    res = {"off": off, "blows": collections.defaultdict(lambda: [0, 0.0, 0, 0.0]),
           "detach": [], "out": [], "win": None, "length": None, "label": label}
    last_hit = {}
    for t, r in ev:
        k = r[0]
        if k in ("C", "X"):
            p, blow = r[2], r[3]
            b = res["blows"][label(p)]
            if blow >= 10:
                b[0] += 1
                b[1] += blow
            else:
                b[2] += 1
                b[3] += blow
            last_hit[p] = (clock(t), blow)
        elif k == "Q":
            comp_ptr = hex(r[3])  # 0x417d70 is the match's: its argument is the component
            res["detach"].append((clock(t), "queued", label(comp_ptr), last_hit.get(comp_ptr)))
        elif k == "F":
            res["detach"].append((clock(t), "done", label(r[2]), None))
        elif k == "T" and len(r) == 4:
            pass
        elif k == "O":
            res["out"].append((clock(t), r[4]))
        elif k == "W":
            res["win"] = (clock(t), r[3], r[4])
    # the T rows carry the clock: the last fighting one is the length
    ts = [r for t, r in ev if is_clock(r) and r[4] == 1]
    res["length"] = max((r[2] for r in ts), default=None)
    res["over"] = next((clock(t) for t, r in ev if is_clock(r) and r[5] == 1), None)
    return res


REASON = {1: "destroyed", 2: "immobilized", 3: "eliminated"}


def table(paths, names=("player 0", "player 1"), cap=120.0):
    """One block for a set of fights: the end (censored at `cap` s of the match clock, the
    ensembles' limit), who is out and why, and the parts lost."""
    import collections as C

    n = 0
    ends, lens, lost, first = C.Counter(), [], C.Counter(), []
    fights_lost = 0
    per_robot = C.Counter()
    for path in paths:
        r = analyse(path)
        if r["off"] is None:
            continue
        n += 1
        out = [(t, k) for t, k in r["out"] if t <= cap]
        det = [d for d in r["detach"] if d[1] == "queued" and d[0] <= cap]
        if out:
            w = r["win"][1] if r["win"] else None
            loser = names[1 - w] if w in (0, 1) else "?"
            ends["%s %s" % (loser, REASON.get(out[0][1], out[0][1]))] += 1
            lens.append(out[0][0])
        else:
            ends["to the clock"] += 1
            lens.append(cap)
        fights_lost += bool(det)
        for d in det:
            lost[d[2]] += 1
            first.append(d[0])
            per_robot[d[2].split("#")[0]] += 1
    first.sort()
    print(f"n={n} fights; mean length {sum(lens) / n:.1f} s (censored at {cap:.0f}); ends: " +
          ", ".join(f"{k} {v}" for k, v in ends.most_common()))
    print(f"parts lost within {cap:.0f} s: {sum(lost.values())} in {fights_lost} fights "
          f"({sum(lost.values()) / n:.2f} per fight), first loss median "
          f"{first[len(first) // 2] if first else 0:.0f} s: " +
          ", ".join(f"{k} {v}" for k, v in lost.most_common()))


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if "--table" in sys.argv:
        # --names=Scout,ALARM: player 0 and 1 of the exhibition
        nm = [a[8:] for a in sys.argv if a.startswith("--names=")]
        table(args, tuple(nm[0].split(",")) if nm else ("player 0", "player 1"))
        return
    summary = "--summary" in sys.argv
    tot = collections.Counter()
    for path in args:
        r = analyse(path)
        name = path.split("/")[-1]
        end = r["out"][0] if r["out"] else None
        print(f"== {name}: length {r['length'] and round(r['length'], 1)} s, out {r['out']}, win {r['win']}")
        det = [d for d in r["detach"] if d[1] == "queued"]
        for d in det:
            print(f"   detach at {d[0]:.1f}: {d[2]}, last blow {d[3]}")
        tot["fights"] += 1
        tot["detached"] += len(det)
        tot["fights_with_detach"] += bool(det)
        if not summary:
            for lab, (n10, s10, n0, s0) in sorted(r["blows"].items()):
                print(f"   {lab:28s} blows>=10: {n10:3d} ({s10:7.1f} hp)  splash<10: {n0:4d} ({s0:6.1f} hp)")
    print(dict(tot))


if __name__ == "__main__":
    main()
