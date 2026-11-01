#!/usr/bin/env python3
"""The Wide Load's bell landing, step by step (docs/29 section 12aj).

    landing_compare.py <dump.jsonl>...

Each argument is a resolver stream of rows `R` (contacts), `B` (every dynamic
body before the resolver), `V` (after it) and `A` (the step piece): a live
`fightcontacts-sentinel-wideload-compressor__live__004..007` capture
(`frida/ra2_fightcontacts.js`), or ours (`RA2_H17_DUMP=<abs path> fidtrace match
...`, the same rows). A step is one `R`; step 0 is the first with the Wide
Load's chassis falling faster than 0.05 m/s (the bell). Per step it prints the
pieces' length (a half is a backstep), and for the Wide Load's chassis, a mini
wheel and the two front N-12 wheels their vertical speed and spin about x
before the resolver, and the number of contacts.

    landing_compare.py --split <dump.jsonl>...   only the steps 0..40 that were split
"""
import json
import sys


def load(path):
    """[{R: contacts, B: {body: row}, V: {body: row}, h: piece/(1/60)}] per resolver call."""
    steps, cur = [], None
    for line in open(path):
        try:
            r = json.loads(line)
        except ValueError:
            continue
        for r in r.get("rows", []) if isinstance(r, dict) else [r]:
            if r[0] == "R":
                cur = {"R": r[3], "B": {}, "V": {}, "h": 1.0}
                steps.append(cur)
            elif cur is None:
                continue
            elif r[0] == "B":
                cur["B"][r[2]] = r
            elif r[0] == "V":
                cur["V"][r[2]] = r
            elif r[0] == "A":
                cur["h"] = r[2] * 60
    return steps


def wide_load_chassis(steps):
    for s in steps:
        for k, b in s["B"].items():
            if b[3] > 150 and b[4][0] < -5 and b[4][1] > 3:
                return k


def bell(steps, chassis):
    return next(i for i, s in enumerate(steps) if chassis in s["B"] and s["B"][chassis][5][1] < -0.05)


def labels(steps, chassis, i):
    """Name the Wide Load's bodies at step i by mass and side (L/R by x, f/b by z)."""
    s = steps[i]
    c0 = s["B"][chassis][4]
    lab = {}
    for k, b in s["B"].items():
        d = [b[4][j] - c0[j] for j in range(3)]
        if k == chassis:
            lab[k] = "CH"
        elif abs(d[0]) < 1.6 and abs(d[2]) < 1.6 and abs(d[1]) < 1.0 and 5 < b[3] < 70:
            lab[k] = "w%.0f%s%s" % (b[3], "L" if d[0] < 0 else "R", "f" if d[2] > 0 else "b")
    return lab


def show(path, only_split=False):
    st = load(path)
    ch = wide_load_chassis(st)
    rel = bell(st, ch)
    lab = labels(st, ch, rel + 12)
    inv = {}
    for k, v in lab.items():
        inv.setdefault(v, k)
    split = [i - rel for i in range(rel, min(rel + 40, len(st))) if st[i]["h"] < 0.99]
    print(path.split("/")[-1], "splits in the first 40 steps:", split)
    if only_split:
        return
    for k in range(9, 18):
        s = st[rel + k]
        row = []
        for nm in ("CH", "w26Lb", "w32Lf", "w32Rf"):
            b = inv.get(nm)
            row.append("%s vy%+.2f wx%+.2f" % (nm, s["B"][b][5][1], s["B"][b][6][0]) if b in s["B"] else nm + " -")
        print("  %2d C%-3d %s" % (k, len(s["R"]), " | ".join(row)))


if __name__ == "__main__":
    a = sys.argv[1:]
    only = "--split" in a
    for p in [x for x in a if not x.startswith("--")]:
        show(p, only)
