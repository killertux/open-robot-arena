#!/usr/bin/env python3
"""Turn live assembly captures into the regression fixture
engine/crates/ra2sim/tests/data/assembly_live.tsv (docs/29 section 8).

    assembly_fixture.py name=AI/Team0/Bot0.bot [name=path ...] > assembly_live.tsv

For every "<object> Instance" node of the robot as the original built it (the
`first` snapshot of local/oracle/original/assembly-<name>__live__000.json) one
row: bot path (relative to the game directory), our component index, object
name, kind and the live position in the chassis frame.

kind is `fixed` for an object whose pose the assembly alone decides, or `spin`
for one on a body a free or spinning hinge carries (a SpinMotor/ServoMotor
rotor, an AxleMount axle, a Steering hub): those bodies are already turning at
the robot's first physics step, so only the distance along and from the hinge
axis is pinned (the test reads the axis from our rig).

The component index comes from matching each live object with our object of the
same name (`asmdump`), nearest first, in the spin-invariant measure for spin
objects.
"""
import json
import math
import os
import subprocess
import sys

import assembly_compare as ac

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
ORIG = os.path.join(ROOT, "local", "oracle", "original")
GAME = os.environ.get("RA2_GAME", os.path.join(ROOT, "local", "game"))
ASMDUMP = os.path.join(ROOT, "engine", "target", "release", "examples", "asmdump")
SPIN = {"SpinMotor", "ServoMotor", "AxleMount", "Steering"}


def colm(a):
    return ac.M4([[a[4 * j + i] for j in range(4)] for i in range(4)])


def invariants(p, joint):
    piv, axis = joint["pivot"], joint["axis"]
    n = math.sqrt(sum(x * x for x in axis))
    axis = [x / n for x in axis]
    d = [p[i] - piv[i] for i in range(3)]
    along = sum(d[i] * axis[i] for i in range(3))
    radial = math.sqrt(max(0.0, sum(x * x for x in d) - along * along))
    return along, radial


def ours(path):
    out = subprocess.run([ASMDUMP, path], capture_output=True, text=True, check=True).stdout
    rows = []
    for c in json.loads(out):
        for o in c["objects"]:
            joint = None
            if o["rotor"]:
                spins = [j for j in c["drives"] if j["motor_class"] in SPIN and not j["slider"]]
                joint = spins[0] if spins else None
            elif c["joint"] and c["joint"]["motor_class"] in SPIN and not c["joint"]["slider"]:
                joint = c["joint"]
            # The original draws a component's own mesh at the component's frame
            # and a body of its own (rotor, steering hub) at its transform.
            m = colm(o["m"]) if (o["rotor"] or o["host"]) else colm(o["frame"])
            rows.append({"name": o["name"], "m": m, "index": c["index"], "joint": joint})
    return rows


def main():
    print("# bot\tcomponent\tobject\tkind\tx\ty\tz")
    for arg in sys.argv[1:]:
        name, rel = arg.split("=", 1)
        cap = json.load(open(os.path.join(ORIG, f"assembly-{name}__live__000.json")))
        live = ac.live_instances(cap, "first")
        mine = ours(os.path.join(GAME, rel))
        used = set()
        for l in live:
            p = l["m"].t
            best = None
            for k, o in enumerate(mine):
                if o["name"] != l["name"] or k in used:
                    continue
                if o["joint"]:
                    a, r = invariants(p, o["joint"])
                    a2, r2 = invariants(o["m"].t, o["joint"])
                    d = math.hypot(a - a2, r - r2)
                else:
                    d = ac.dist(p, o["m"].t)
                if best is None or d < best[0]:
                    best = (d, k)
            if best is None:
                continue  # an object we do not draw (the smart zone)
            used.add(best[1])
            o = mine[best[1]]
            kind = "spin" if o["joint"] else "fixed"
            print(f"{rel}\t{o['index']}\t{l['name']}\t{kind}\t{p[0]:.4f}\t{p[1]:.4f}\t{p[2]:.4f}")


if __name__ == "__main__":
    main()
