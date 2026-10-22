#!/usr/bin/env python3
"""Spec for `ra2sim --example decalshot` from a capture_fx.py `SYNTH=1` run (docs/37 section 8).

  slash_spec.py fx-<name>__live__<seq>.jsonl <bot0> <bot1> <index> <theta degrees> [alpha] > spec.txt

One `spair` row: the decal our hit routine call made is the decal centre `point` and normal `normal` (in the
struck chassis' frame, from the matrix the capture read), size 0.5 x 0.5, mode 1 (the weapon definition's
`single_slash.tga`); its width axis is not observable, so it is turned `theta` about the normal from an
arbitrary perpendicular.  The robot that was struck is player 1.
"""
import json
import math
import sys


def main():
    rows = [json.loads(l) for l in open(sys.argv[1])]
    bots = sys.argv[2:4]
    r = [x for x in rows if x["ev"] == "spair"][int(sys.argv[4])]
    theta = math.radians(float(sys.argv[5]))
    alpha = float(sys.argv[6]) if len(sys.argv) > 6 else r["damage"] / 100.0
    (addr, m), = r["chassis"].items()
    ax = [m[0:3], m[3:6], m[6:9]]
    # world -> local for a rigid matrix whose rows are the axes: local_j = (p - t) . ax_j
    loc = lambda p: [sum((p[k] - m[9 + k]) * ax[j][k] for k in range(3)) for j in range(3)]
    rot = lambda v: [sum(v[k] * ax[j][k] for k in range(3)) for j in range(3)]
    c = loc(r["point"])
    n = rot(r["normal"])
    # a perpendicular to n
    a = [0, 1, 0] if abs(n[1]) < 0.9 else [1, 0, 0]
    e1 = [a[1] * n[2] - a[2] * n[1], a[2] * n[0] - a[0] * n[2], a[0] * n[1] - a[1] * n[0]]
    l1 = math.sqrt(sum(x * x for x in e1))
    e1 = [x / l1 for x in e1]
    e2 = [n[1] * e1[2] - n[2] * e1[1], n[2] * e1[0] - n[0] * e1[2], n[0] * e1[1] - n[1] * e1[0]]
    axis = [math.cos(theta) * e1[i] + math.sin(theta) * e2[i] for i in range(3)]
    f = lambda v: " ".join("%.6f" % x for x in v)
    for p, b in enumerate(bots):
        print(f"bot {p} {b}")
    print(f"mark 1 single_slash.tga 1 0.5 0.5 {alpha:.6f} {f(c)} {f(n)} {f(axis)}")
    cam = eval(r["cam"])
    far = list(m)
    far[9] += 30.0
    print("shot slash 1 %.6f %.6f %.6f %.6f %.6f %.4f %s %s" % (*cam[0], cam[1], cam[2], r["fov"], f(far), f(m)))


if __name__ == "__main__":
    main()
