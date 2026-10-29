#!/usr/bin/env python3
"""How far the ORIGINAL turns every hinged body of a robot about its hinge
between the build and the hold (docs/29 section 12w), from a
`spinup-*__live__*.jsonl` capture (frida/ra2_spinup.js through capture_rest.py
with WATCH_AGENT=ra2_spinup.js).

    spincmp.py <capture.jsonl> [ours.txt]
    spincmp.py frames <capture.jsonl>    FRAME lines for examples/orderprobe.rs

Per robot (a system with a body of 20 kg or more): the substeps between its
placement (0x4dcff0) and its lowering (0x418b90), and per body its centre
relative to the chassis' at the build (robot frame), its mass and the turn
from the build to the hold in the robot's frame as an angle and axis (`P`
rows, the same as `examples/spinprobe.rs` prints). With `ours.txt` (spinprobe's
output) every live body is matched with ours by mass and centre and the angles
are printed side by side, signed about our hinge's axis.
"""
import json
import math
import sys


def qmat(q):
    w, x, y, z = q
    return [[1 - 2 * (y * y + z * z), 2 * (x * y - w * z), 2 * (x * z + w * y)],
            [2 * (x * y + w * z), 1 - 2 * (x * x + z * z), 2 * (y * z - w * x)],
            [2 * (x * z - w * y), 2 * (y * z + w * x), 1 - 2 * (x * x + y * y)]]


def mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def tr(a):
    return [[a[j][i] for j in range(3)] for i in range(3)]


def mv(a, v):
    return [sum(a[i][k] * v[k] for k in range(3)) for i in range(3)]


def axang(r):
    c = (r[0][0] + r[1][1] + r[2][2] - 1) / 2
    ang = math.acos(max(-1.0, min(1.0, c)))
    ax = [r[2][1] - r[1][2], r[0][2] - r[2][0], r[1][0] - r[0][1]]
    n = math.sqrt(sum(a * a for a in ax))
    if n < 1e-9:
        # 0 or half a turn: the axis from the symmetric part.
        d = [math.sqrt(max(0.0, (r[i][i] + 1) / 2)) for i in range(3)]
        k = max(range(3), key=lambda i: d[i])
        ax = [(r[k][i] + r[i][k]) / 2 / max(d[k], 1e-9) if i != k else d[k] for i in range(3)]
        n = math.sqrt(sum(a * a for a in ax)) or 1.0
    return ang, [a / n for a in ax]


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


def robots(rows):
    """[(system, [bodies])] in the order the systems were seen."""
    out = {}
    for r in rows:
        if r[0] == "Y" and any(b[1] >= 20 for b in r[3]):
            out[r[2]] = [b[0] for b in r[3]]
    return list(out.items())


def analyse(path):
    rows = load(path)
    by_step = {}
    for r in rows:
        if r[0] == "B":
            by_step.setdefault(r[1], {})[r[2]] = r
    marks = [r for r in rows if r[0] in ("P", "L")]
    result = []
    for sys_, bodies in robots(rows):
        # Only a robot's system: more than one body (an arena's held body is
        # a system alone), and only one the match places (below).
        if len(bodies) < 2:
            continue
        chassis = None
        # The chassis: the heaviest body.
        first = {}
        for n in sorted(by_step):
            for b in bodies:
                if b in by_step[n] and b not in first:
                    first[b] = by_step[n][b]
        if not first:
            continue
        masses = {}
        for r in rows:
            if r[0] == "Y" and r[2] == sys_:
                for b in r[3]:
                    masses[b[0]] = b[1]
        chassis = max(bodies, key=lambda b: masses.get(b, 0))
        # The placement: the step at which the chassis' heading first changes,
        # the lowering: the first step the chassis is flagged held (+0x1c 4)
        # after it.
        steps = sorted(n for n in by_step if chassis in by_step[n])
        place = next((n for n in steps if abs(by_step[n][chassis][5][0] - first[chassis][5][0]) > 1e-4
                      or abs(by_step[n][chassis][5][2] - first[chassis][5][2]) > 1e-4), None)
        if place is None:
            continue
        # The hold: the step after the lowering (0x418b90) that follows the
        # placement.
        lowers = [m[1] for m in marks if m[0] == "L" and m[2] == "leave" and m[1] >= place]
        if not lowers or lowers[0] + 1 not in by_step:
            continue
        hold = lowers[0] + 1
        if any(b not in by_step[hold] for b in bodies):
            continue
        # As built: the step before the placement, every body complete.
        pre = by_step.get(place - 1, {})
        if any(b not in pre for b in bodies):
            continue
        build = {b: pre[b] for b in bodies}
        held = {b: by_step[hold][b] for b in bodies}
        c0 = qmat(build[chassis][5])
        c1 = qmat(held[chassis][5])
        lowered = [m for m in marks if m[0] == "L" and m[2] == "enter" and m[1] >= place]
        res = {"system": sys_, "place": place, "hold": hold, "chassis_mass": masses.get(chassis, 0),
               "lower": lowered[0][1] if lowered else None, "chassis": chassis, "bodies": []}
        for b in bodies:
            if b == chassis:
                continue
            r0 = qmat(build[b][5])
            r1 = qmat(held[b][5])
            # Robot frame: the chassis' at the build.
            # The turn from the build to the hold relative to the chassis, in
            # the chassis' frame: C1^T R1 (C0^T R0)^T.
            rel = mul(mul(tr(c1), r1), mul(tr(r0), c0))
            ang, ax = axang(rel)
            d = mv(tr(c0), [build[b][4][i] - build[chassis][4][i] for i in range(3)])
            # The body's vertical speed after the first substep from the
            # placement: -0.2 m/s after a whole one (gravity 12 m/s^2), less
            # when the world took that substep in pieces.
            first_vy = by_step[place + 1][b][6][1] if place + 1 in by_step and b in by_step[place + 1] else None
            res["bodies"].append({"body": b, "mass": masses.get(b, 0), "d": d, "angle": ang, "axis": ax,
                                  "first_vy": first_vy})
        result.append(res)
    return result


def read_ours(path):
    out = []
    chassis = {}
    for line in open(path):
        w = line.split()
        if w and w[0] == "R":
            k = w.index("chassis") if "chassis" in w else -1
            chassis[int(w[1])] = float(w[k + 1]) if k >= 0 else 0.0
        if w and w[0] == "P":
            v = [float(x) for x in w[3:11]]
            out.append({"robot": int(w[1]), "part": int(w[2]), "mass": v[0], "d": v[1:4], "angle": v[4],
                        "axis": v[5:8], "chassis": chassis.get(int(w[1]), 0.0)})
    return out


def frames(path):
    """`FRAME` lines for examples/orderprobe.rs: every robot body as built (the
    step before its placement), its centre from its chassis' and its rotation
    (w x y z), the chassis unturned."""
    rows = load(path)
    by_step = {}
    for r in rows:
        if r[0] == "B":
            by_step.setdefault(r[1], {})[r[2]] = r
    for res in analyse(path):
        pre = by_step[res["place"] - 1]
        ch = pre[res["chassis"]]
        for b in [res["chassis"]] + [x["body"] for x in res["bodies"]]:
            d = [pre[b][4][i] - ch[4][i] for i in range(3)]
            print("FRAME %s %s" % (" ".join(repr(x) for x in d), " ".join(repr(x) for x in pre[b][5])))


def main():
    if sys.argv[1] == "frames":
        frames(sys.argv[2])
        return
    live = sorted(analyse(sys.argv[1]), key=lambda r: r["place"])
    ours = read_ours(sys.argv[2]) if len(sys.argv) > 2 else []
    for ri, r in enumerate(live):
        def fit(k):
            mine = [o for o in ours if o["robot"] == k]
            if not mine:
                return float("inf")
            return sum(min(math.dist(o["d"], b["d"]) for o in mine) for b in r["bodies"])
        robots_ours = sorted({o["robot"] for o in ours})
        best_robot = min(robots_ours, key=fit) if robots_ours else None
        print(f"robot {ri} ({r['chassis_mass']:.1f} kg): placed at step {r['place']}, lowered {r['lower']}, held {r['hold']}")
        for b in r["bodies"]:
            line = (f"  P {ri} {b['body']} {b['mass']:.2f} {b['d'][0]:.3f} {b['d'][1]:.3f} {b['d'][2]:.3f}"
                    f"  live {math.degrees(b['angle']):7.2f} deg about ({b['axis'][0]:.2f} {b['axis'][1]:.2f} {b['axis'][2]:.2f})"
                    f" vy1 {b['first_vy'] if b['first_vy'] is None else round(b['first_vy'], 3)}")
            # Our robot whose bodies sit where this one's do (the placing order
            # may differ from ours).
            cands = [o for o in ours if o["robot"] == best_robot]
            if cands:
                o = min(cands, key=lambda o: sum((o["d"][i] - b["d"][i]) ** 2 for i in range(3)))
                # Both as turns about the live turn's axis, in (-180, 180].
                s = 1 if sum(o["axis"][i] * b["axis"][i] for i in range(3)) >= 0 else -1
                ours_deg = (s * math.degrees(abs(o["angle"])) + 180) % 360 - 180
                live_deg = (math.degrees(b["angle"]) + 180) % 360 - 180
                line += f"  ours {ours_deg:7.2f} deg (part {o['part']}), ours - live {(ours_deg - live_deg + 180) % 360 - 180:7.2f}"
            print(line)


if __name__ == "__main__":
    main()
