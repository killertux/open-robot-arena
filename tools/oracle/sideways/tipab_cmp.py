#!/usr/bin/env python3
"""tipab_cmp.py <live.jsonl> <ours.jsonl>: the end state of each trial of a tipab grid (frida/ra2_tipab.js, capture_tipab.py) in the
original and in ours (ours_tipab.py), side by side (docs/29 12bd).

Classes of the Scout's chassis at the end: T on its tail (its forward axis vertical), U on its wheels, I on its back. The chassis' B row
carries the rotation as three rows, each a chassis axis in the world: row 1 (index 3..5) is the chassis' up axis, row 2 the forward axis."""
import json
import sys

MASS = 163.495  # the Scout's chassis


def trials(path):
    out, cur = [], None
    for line in open(path):
        r = json.loads(line)
        if r[0] == "TRIAL":
            cur = {"label": r[2].get("label"), "rows": []}
            out.append(cur)
        elif r[0] == "B" and abs(r[3] - MASS) < 0.01 and cur is not None:
            cur["rows"].append(r)
    return out


def classify(t):
    if not t["rows"]:
        return "?"
    m = t["rows"][-1][7]
    if m[4] < -0.5:
        return "I"
    if abs(m[7]) > 0.9:
        return "T"
    if m[4] > 0.5:
        return "U"
    return "?"


def table(path):
    return {t["label"]: classify(t) for t in trials(path)}


if __name__ == "__main__":
    live, ours = table(sys.argv[1]), table(sys.argv[2])
    bad = 0
    for k, v in live.items():
        o = ours.get(k, "-")
        bad += v != o
        print("%-28s live %s ours %s%s" % (k, v, o, "" if v == o else "   <-"))
    print("different: %d of %d" % (bad, len(live)))
