#!/usr/bin/env python3
"""How the robots land at the bell, live against ours (docs/29 sections 12w, 12x).

    landcmp.py <spinup-deact-*.jsonl | rest deact capture> [ours.txt]

Live: every robot body every substep from the hold (frida/ra2_deact.js rows
`B`). Per robot (chassis = the heaviest body of a system): from the first
substep the chassis moves, the lowest drop of its centre within 0.5 s, the
rebound after that low point within the 0.5 s, the yaw change at 0.5 s, and
the fastest downward speed of any of its other bodies (the wheels pulled down
through their hinges). `ours.txt` is `examples/spinprobe` output with `LAND=`
(`L <step> <robot> <y/vy>...` chassis first, and `Q <step> <robot> <yaw>`).
"""
import json
import math
import sys

WINDOW = 30  # substeps: 0.5 s


def load(path):
    rows = []
    for line in open(path):
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if isinstance(r, dict):
            rows.extend(r.get("rows", []))
        elif isinstance(r, list):
            rows.append(r)
    return rows


def yaw(q):
    w, x, y, z = q
    # The heading of the body's +z axis about the vertical.
    fx = 2 * (x * z + w * y)
    fz = 1 - 2 * (x * x + y * y)
    return math.atan2(fx, fz)


def wrap(a):
    return (a + math.pi) % (2 * math.pi) - math.pi


def summarise(ys, yaws, others):
    """ys: chassis centre heights per substep from the release, yaws the
    chassis heading, others: per substep the vertical speeds of the other
    bodies."""
    w = min(WINDOW, len(ys) - 1)
    y0 = ys[0]
    lo = min(range(w + 1), key=lambda k: ys[k])
    drop = y0 - ys[lo]
    rebound = max(ys[k] for k in range(lo, w + 1)) - ys[lo]
    turn = wrap(yaws[w] - yaws[0])
    pull = -min(min(v) for v in others[: w + 1] if v) if any(others[: w + 1]) else 0.0
    return drop, rebound, turn, pull


def live(path):
    rows = load(path)
    systems = {}
    for r in rows:
        if r[0] == "Y" and r[3] and len(r[3]) > 1 and max(b[1] for b in r[3]) >= 20:
            systems[r[2]] = r[3]
    by = {}
    for r in rows:
        if r[0] == "B":
            by.setdefault(r[1], {})[r[2]] = r
    steps = sorted(by)
    out = []
    for s, bodies in systems.items():
        ch = max(bodies, key=lambda b: b[1])[0]
        rest = [b[0] for b in bodies if b[0] != ch]
        seq = [n for n in steps if ch in by[n]]
        if not seq:
            continue
        y_hold = by[seq[0]][ch][4][1]
        start = next((i for i, n in enumerate(seq) if abs(by[n][ch][4][1] - y_hold) > 1e-6), None)
        if start is None:
            continue
        start = max(0, start - 1)
        seq = seq[start:start + WINDOW + 1]
        ys = [by[n][ch][4][1] for n in seq]
        yaws = [yaw(by[n][ch][5]) for n in seq]
        others = [[by[n][b][6][1] for b in rest if b in by[n]] for n in seq]
        out.append((max(b[1] for b in bodies), summarise(ys, yaws, others)))
    return out


def ours(path):
    L, Q = {}, {}
    for line in open(path):
        w = line.split()
        if not w:
            continue
        if w[0] == "L":
            L.setdefault(int(w[2]), []).append([tuple(float(x) for x in c.split("/")) for c in w[3:]])
        elif w[0] == "Q":
            Q.setdefault(int(w[2]), []).append(float(w[3]))
    out = []
    for r in sorted(L):
        seq = L[r]
        y_hold = seq[0][0][0]
        start = next((i for i, s in enumerate(seq) if abs(s[0][0] - y_hold) > 1e-6), None)
        if start is None:
            continue
        start = max(0, start - 1)
        s = seq[start:start + WINDOW + 1]
        q = Q.get(r, [0.0] * len(seq))[start:start + WINDOW + 1]
        ys = [c[0][0] for c in s]
        others = [[v for _, v in c[1:]] for c in s]
        out.append((r, summarise(ys, q, others)))
    return out


def main():
    lv = live(sys.argv[1])
    for m, (d, rb, t, p) in lv:
        print(f"live  chassis {m:6.1f} kg: drop {d:.3f} rebound {rb:.3f} yaw {t:+.3f} wheels down {p:.2f} m/s")
    if len(sys.argv) > 2:
        for r, (d, rb, t, p) in ours(sys.argv[2]):
            print(f"ours  robot {r}: drop {d:.3f} rebound {rb:.3f} yaw {t:+.3f} wheels down {p:.2f} m/s")


if __name__ == "__main__":
    main()
