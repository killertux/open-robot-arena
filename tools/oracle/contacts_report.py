#!/usr/bin/env python3
"""Summarise a contacts capture (capture_contacts.py) for one robot.

usage: contacts_report.py <contacts-*.jsonl> [chassis_mass] [step_s]

For the robot whose chassis body has `chassis_mass` (default: the lightest
chassis above 100 kg), prints from the thrust stop after its fastest straight:
time, chassis speed, and per wheel body (the 27 kg bodies nearest the chassis)
the number of contact points against the arena, their distances, and the spin
about the body's axle against the rolling spin.  Also the step pieces per
Havok substep (backsteps).
"""
import json
import math
import sys


def load(path):
    rows = [json.loads(line) for line in open(path)]
    pieces = []  # (t, h, rollbacks, contacts, bodies, magic)
    t = 0.0
    cur = None
    magic = {}
    for r in rows:
        if r[0] == 'R':
            cur = {'contacts': r[3], 'bodies': {}, 'tol': r[2]}
        elif r[0] == 'B' and cur is not None:
            cur['bodies'][r[2]] = {'mass': r[3], 'com': r[4], 'vel': r[5], 'w': r[6], 'rot': r[7]}
        elif r[0] == 'G':
            magic[r[2]] = r[3]
        elif r[0] == 'A':
            if cur is not None:
                pieces.append((t, r[2], r[3], cur, dict(magic)))
            t += r[2]
            cur = None
    return pieces


def main():
    pieces = load(sys.argv[1])
    want = float(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[2] != '-' else None
    step = float(sys.argv[3]) if len(sys.argv) > 3 else 0.1
    masses = {}
    for _, _, _, c, _ in pieces:
        for k, b in c['bodies'].items():
            masses[k] = b['mass']
    chassis = [k for k, m in masses.items() if m > 100]
    if want is not None:
        chassis = [k for k in chassis if abs(masses[k] - want) < 0.5]
    chassis = min(chassis, key=lambda k: masses[k])
    # thrust per step for this chassis' MagicAction
    series = []
    for t, h, rb, c, mg in pieces:
        b = c['bodies'].get(chassis)
        thrust = mg.get(chassis)
        if b is None or thrust is None:
            continue
        series.append((t, h, rb, c, b, thrust))
    k = max(range(len(series)), key=lambda i: math.hypot(series[i][4]['vel'][0], series[i][4]['vel'][2]))
    i = k
    while i < len(series) and any(w[0] != 0 for w in series[i][5]):
        i += 1
    t0 = series[i][0]
    c0 = series[i][4]['com']
    # wheels: 27 kg bodies within 1 m of the chassis at the stop
    wheels = [kk for kk, b in series[i][3]['bodies'].items()
              if abs(b['mass'] - 27) < 0.5 and math.dist(b['com'], c0) < 1.0]
    print(f"# chassis {chassis} ({masses[chassis]:.2f} kg), wheels {wheels}, stop at {t0:.2f}")
    nxt = t0 - 1.0
    for t, h, rb, c, b, thrust in series:
        if t < t0 - 1.0 or t > t0 + 3.05 or t < nxt - 1e-9:
            continue
        nxt += step
        v = b['vel']
        line = f"{t - t0:6.2f} v {math.hypot(v[0], v[2]):6.2f} y {b['com'][1]:7.3f} h {h:.4f} rb {rb}"
        for wk in wheels:
            wb = c['bodies'].get(wk)
            pts = [x for x in c['contacts'] if wk in (x[0], x[1]) and masses.get(x[0] if x[1] == wk else x[1], 0) == 0]
            ds = ' '.join(f"{x[4]:.3f}" for x in pts)
            spin = ''
            if wb:
                rot = wb['rot']
                # the hub's axle: the body-frame axis with the least velocity
                axes = [(rot[0], rot[1], rot[2]), (rot[3], rot[4], rot[5]), (rot[6], rot[7], rot[8])]
                w = wb['w']
                ax = max(axes, key=lambda a: abs(sum(p * q for p, q in zip(a, w))))
                s = sum(p * q for p, q in zip(ax, w))
                roll = math.hypot(wb['vel'][0], wb['vel'][2]) / 0.185
                spin = f" w {abs(s):5.1f}/{roll:5.1f}"
            line += f" | {len(pts)}p [{ds}]{spin}"
        print(line)


if __name__ == '__main__':
    main()
