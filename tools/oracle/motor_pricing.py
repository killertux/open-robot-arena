#!/usr/bin/env python3
"""motor_pricing.py <colliderdump> <game> <scout bot> <alarm bot> <capture.jsonl>...: how ALARM's hammer
head's contacts with each piece of the Scout reach the event filter and the price list, live
(docs/29 section 12at). Needs captures of frida/ra2_robotcolliders.js with its F rows (0x52d4f0, one per
event the world asks the filter about, between two robot collidables, first 30 s of the match, with the
contact's distance) and H rows (0x417330, every priced hit).

Per Scout piece, the contacts of the hammer head's piece: filter calls, those it let through, the
smallest distance, and the hits priced on the piece's component by the hammer (count, by size).
"""
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import collider_compare as cc
import parts_live as pl


def main():
    dump, game, scout, alarm = sys.argv[1:5]
    tot = collections.defaultdict(lambda: [0, 0, 9.0, 0, 0, 0.0])
    for path in sys.argv[5:]:
        rows = cc.load(path)
        names, comps = cc.tag_names(rows, dump, game, [scout, alarm])
        sl, al = scout.split("/")[-1][:-4], alarm.split("/")[-1][:-4]
        # K: component pointer -> (robot, id), the robot by the chassis' vertex count of its bodies
        g = cc.groups(rows)
        chassis_nv = {}
        for r in rows:
            if r[0] == "C" and r[2] == 4 and r[5] >= 20 and r[4] in g:
                chassis_nv[g[r[4]]] = r[5]
        who = {}
        for r in rows:
            if r[0] != "K":
                continue
            bs = [w[1] for w in r[5] if w[1] in g] + [w[2] for w in r[5] if w[2] in g]
            if bs and g[bs[0]] in chassis_nv:
                who[r[4]] = ("Scout" if chassis_nv[g[bs[0]]] == 36 else "ALARM", r[3])
        hammer = next((t for t, n in names.items() if n == f"{al}:Hammer Head"), None)
        # the hammer's tag changes when the rod is rebuilt: every tag named so
        hammers = {t for t, n in names.items() if n == f"{al}:Hammer Head"}
        for r in rows:
            if r[0] == "F" and ((r[2] in hammers and comps.get(r[3], ("",))[0] == sl) or (r[3] in hammers and comps.get(r[2], ("",))[0] == sl)):
                other = r[3] if r[2] in hammers else r[2]
                q = tot[comps[other][1]]
                q[0] += 1
                q[1] += r[4]
                q[2] = min(q[2], r[5])
        for r in rows:
            if r[0] == "H" and who.get(r[3], ("",))[0] == "ALARM" and who.get(r[5], ("",))[0] == "Scout":
                att = pl.ROBOTS[2662][1][who[r[3]][1]]
                if att == "Hammer Head":
                    q = tot[who[r[5]][1]]
                    q[3] += 1
                    q[4] += r[6] >= 10
                    q[5] += r[6]
    print("per Scout component (index): filter calls with the hammer head's piece, passed, min distance, priced hits, of 10 hp or more, hp")
    for c, (n, p, d, h, hb, hp) in sorted(tot.items()):
        print(f"  {c:2d} {pl.ROBOTS[1828][1][c]:16s} calls {n:5d} passed {p:4d} min distance {d:.3f}   priced {h:3d} (>=10: {hb:3d}) hp {hp:6.0f}")


if __name__ == "__main__":
    main()
