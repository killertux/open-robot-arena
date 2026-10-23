#!/usr/bin/env python3
"""Width axis of a synthetic slash decal from the polygon the original cut (docs/37 section 8.1).

  decal_axis.py fx-slash-synth2__live__000.jsonl <spair index>

`capture_fx.py SYNTH=1` rows `spair` carry `found`: the DecalTrigger object located in memory after the hit routine call
(`frida/ra2_fx.js finddecals`: cut positions `pos` in the chassis' frame and `uv`).  uv = ((p - c) . A / w + .5, (p - c) . V / h + .5), so a least squares
fit of the uvs against the positions, in the plane of the decal, gives A and V.  Prints their angles about the normal from
`e1 = (0,1,0) x N` (the convention of slash_spec.py, whose `theta` this is) and their in-plane lengths.
"""
import json
import math
import sys

rows = [json.loads(l) for l in open(sys.argv[1])]
r = [x for x in rows if x["ev"] == "spair"][int(sys.argv[2])]
f = r["found"][0]
(addr, m), = r["chassis"].items()
ax = [m[0:3], m[3:6], m[6:9]]
loc = lambda p: [sum((p[k] - m[9 + k]) * ax[j][k] for k in range(3)) for j in range(3)]
rot = lambda v: [sum(v[k] * ax[j][k] for k in range(3)) for j in range(3)]
dot = lambda a, b: sum(x * y for x, y in zip(a, b))
cross = lambda a, b: [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
c, n = loc(r["point"]), rot(r["normal"])
a = [0, 1, 0] if abs(n[1]) < 0.9 else [1, 0, 0]
e1 = cross(a, n)
l = math.sqrt(dot(e1, e1))
e1 = [x / l for x in e1]
e2 = cross(n, e1)
P, UV, w, h = f["pos"], f["uv"], 0.5, 0.5
X = [(dot([p[k] - c[k] for k in range(3)], e1), dot([p[k] - c[k] for k in range(3)], e2), 1.0) for p in P]


def lsq(s):
    A = [[sum(x[i] * x[j] for x in X) for j in range(3)] for i in range(3)]
    b = [sum(x[i] * s[t] for t, x in enumerate(X)) for i in range(3)]
    for i in range(3):
        piv = max(range(i, 3), key=lambda q: abs(A[q][i]))
        A[i], A[piv], b[i], b[piv] = A[piv], A[i], b[piv], b[i]
        for q in range(i + 1, 3):
            t = A[q][i] / A[i][i]
            for j in range(i, 3):
                A[q][j] -= t * A[i][j]
            b[q] -= t * b[i]
    x = [0.0] * 3
    for i in (2, 1, 0):
        x[i] = (b[i] - sum(A[i][j] * x[j] for j in range(i + 1, 3))) / A[i][i]
    return x, max(abs(dot(X[t], x) - s[t]) for t in range(len(X)))


xu, ru = lsq([(uv[0] - .5) * w for uv in UV])
xv, rv = lsq([(uv[1] - .5) * h for uv in UV])
print("vertices", len(P), "fit residual", max(ru, rv))
print("width axis A: theta %.2f deg, in-plane length %.4f" % (math.degrees(math.atan2(xu[1], xu[0])), math.hypot(xu[0], xu[1])))
print("height axis V: angle %.2f deg, in-plane length %.4f" % (math.degrees(math.atan2(xv[1], xv[0])), math.hypot(xv[0], xv[1])))
