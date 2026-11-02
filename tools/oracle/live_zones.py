#!/usr/bin/env python3
"""live_zones.py <boxfight-*.jsonl ...>: the arena zones' events and the robots' flights in
a live fight captured with frida/ra2_boxfight.js (docs/29 12am).

Per capture: the sim time (sum of the A rows) of every ZoneEvent (W rows, robot
1-based: 1 Scout, 2 ALARM, zone id 1 / 2 the hellraisers, 3 / 4 the hammers) as
runs, and every spell of a chassis above 1.0 m (X rows: chassis centre).
"""
import json, sys

def load(path):
    t = 0.0
    ev, fl = [], {}
    chas = {}
    for line in open(path):
        r = json.loads(line)
        if r[0] == "A":
            t += r[2]
        elif r[0] == "W":
            ev.append((t, r[2][1], r[2][2], r[2][0]))
        elif r[0] == "X":
            for c, com, rot in r[2]:
                fl.setdefault(c, []).append((t, com))
    return ev, fl

for path in sys.argv[1:]:
    ev, fl = load(path)
    print(path.rsplit("/", 1)[-1])
    runs = []
    for t, z, rob, d in ev:
        if runs and runs[-1][1] == z and runs[-1][2] == rob and t - runs[-1][4] < 0.5:
            runs[-1][4] = t; runs[-1][5] += 1
        else:
            runs.append([t, z, rob, d, t, 1])
    for t0, z, rob, d, t1, n in runs:
        print(f"  zone {z} robot {rob} {t0:7.2f}..{t1:7.2f} ({n} events)")
    # chassis pointers: order of first appearance = robot 1 (Scout?), robot 2 - print both
    for k, (c, tr) in enumerate(fl.items()):
        peak = max(tr, key=lambda p: p[1][1])
        air = [t for t, com in tr if com[1] > 1.0]
        print(f"  chassis {c}: peak y {peak[1][1]:.2f} at {peak[0]:.2f}; above 1 m: {len(air)} samples" + (f" first {air[0]:.2f}" if air else ""))
