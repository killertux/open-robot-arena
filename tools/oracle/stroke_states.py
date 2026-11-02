#!/usr/bin/env python3
"""stroke_states.py <live jsonl> <ours b0.txt (FIDTRACE_BODY at the bell)> <clock s> > state

Turns the `B` rows of frida/ra2_firststroke.js (every Havok body of both robots, every
step piece: centre of mass, rotation +0xcc as 3 rows = the body's axes in world, velocity,
angular velocity) into the state file `fidtrace replay` reads (docs/29 section 12as):
`robot part com(3) quat(x y z w) v(3) w(3)`.  Live bodies are matched to ours by their
position in their chassis' frame at the first row; the chassis by mass (Scout 163.49,
ALARM 232.1).  `clock` is the match clock; the nearest piece at or before it is used.
"""
import json
import math
import sys


def rows(path):
    t = 0.0
    clk = None
    bp = {}
    out = []
    for l in open(path):
        r = json.loads(l)
        k = r[0]
        if k == "A":
            t += r[2]
        elif k == "MC":
            clk = (r[2], t)
        elif k == "BP":
            bp[r[2]] = r[4][0x190 // 4]
        elif k == "B" and clk:
            out.append((clk[0] + (t - clk[1]), {x[0]: x for x in r[2]}))
    return bp, out


def mat(x):
    return [x[4:7], x[7:10], x[10:13]]


def to_local(m, v):  # local = M world
    return [sum(m[i][j] * v[j] for j in range(3)) for i in range(3)]


def quat_from_world_from_local(M):
    # R = M^T (world <- local)
    R = [[M[j][i] for j in range(3)] for i in range(3)]
    tr = R[0][0] + R[1][1] + R[2][2]
    if tr > 0:
        s = math.sqrt(tr + 1) * 2
        return ((R[2][1] - R[1][2]) / s, (R[0][2] - R[2][0]) / s, (R[1][0] - R[0][1]) / s, 0.25 * s)
    if R[0][0] > R[1][1] and R[0][0] > R[2][2]:
        s = math.sqrt(1 + R[0][0] - R[1][1] - R[2][2]) * 2
        return (0.25 * s, (R[0][1] + R[1][0]) / s, (R[0][2] + R[2][0]) / s, (R[2][1] - R[1][2]) / s)
    if R[1][1] > R[2][2]:
        s = math.sqrt(1 + R[1][1] - R[0][0] - R[2][2]) * 2
        return ((R[0][1] + R[1][0]) / s, 0.25 * s, (R[1][2] + R[2][1]) / s, (R[0][2] - R[2][0]) / s)
    s = math.sqrt(1 + R[2][2] - R[0][0] - R[1][1]) * 2
    return ((R[0][2] + R[2][0]) / s, (R[1][2] + R[2][1]) / s, 0.25 * s, (R[1][0] - R[0][1]) / s)


def ours_bodies(path):
    first = {}
    for l in open(path):
        a = l.split()
        if float(a[1]) != 0.0:
            break
        first[(int(a[2]), int(a[3]))] = list(map(float, a[4:]))
    return first


def qrot(q, v):
    x, y, z, w = q
    t = [2 * (y * v[2] - z * v[1]), 2 * (z * v[0] - x * v[2]), 2 * (x * v[1] - y * v[0])]
    return [v[0] + w * t[0] + (y * t[2] - z * t[1]), v[1] + w * t[1] + (z * t[0] - x * t[2]),
            v[2] + w * t[2] + (x * t[1] - y * t[0])]


def main():
    bp, rs = rows(sys.argv[1])
    ours = ours_bodies(sys.argv[2])
    T = float(sys.argv[3])
    chassis = {}
    for i, m in bp.items():
        if abs(m - 163.49) < 1:
            chassis[0] = i
        if abs(m - 232.1) < 1:
            chassis[1] = i
    s0 = rs[0][1]
    # match by chassis-frame position
    def rel_live(s, ri, i):
        c = s[chassis[ri]]
        return to_local(mat(c), [s[i][1 + k] - c[1 + k] for k in range(3)])

    def rel_ours(ri, pi):
        c = ours[(ri, 0)]
        q = (-c[3], -c[4], -c[5], c[6])
        return qrot(q, [ours[(ri, pi)][k] - c[k] for k in range(3)])

    mapping = {}
    for (ri, pi) in sorted(ours):
        if pi == 0:
            mapping[(ri, 0)] = chassis[ri]
            continue
        ro = rel_ours(ri, pi)
        best = min((i for i in s0 if i not in chassis.values()),
                   key=lambda i: sum((a - b) ** 2 for a, b in zip(rel_live(s0, ri, i), ro)))
        d = math.dist(rel_live(s0, ri, best), ro)
        sys.stderr.write(f"ours r{ri} p{pi} <- live {best} (off {d:.3f} m)\n")
        mapping[(ri, pi)] = best
    t, s = max(((t, s) for t, s in rs if t <= T), key=lambda x: x[0])
    sys.stderr.write(f"state at clock {t:.3f}\n")
    for (ri, pi), i in sorted(mapping.items()):
        x = s[i]
        q = quat_from_world_from_local(mat(x))
        print(ri, pi, *x[1:4], *q, *x[13:16], *x[16:19])


main()
