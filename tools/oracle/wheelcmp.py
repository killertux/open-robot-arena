#!/usr/bin/env python3
"""A Steering unit's wheel constraints, live against ours, call by call
(docs/29 section 12y).

    wheelcmp.py calls <wheel-capture.jsonl>       > calls.txt   (examples/wheelreplay's input)
    wheelcmp.py cmp <wheel-capture.jsonl> <ours.txt>             (wheelreplay's output)
    wheelcmp.py track <wheel-capture.jsonl>                      per substep: steer, travel, hub spin

From `frida/ra2_wheel.js`'s WS/WE rows: every solver call's set-up (0x553c30)
and passes (0x5547d0) of each wheel atom, the two bodies at the set-up's entry,
the velocities at every pass's entry and at every exit. `cmp` prints, per
stage, the change the live call made to the bodies' velocities and ours from
the same entry (the impulse it applied, as a velocity change), and sums
the misses.
"""
import json
import math
import sys


def load(path):
    rows = []
    for line in open(path):
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if isinstance(r, dict) and "rows" in r:
            rows += r["rows"]
        elif isinstance(r, list):
            rows.append(r)
    return rows


def calls(rows):
    """{(n, atom, seq): {'s': (ws, we), 'p': [(ws, we), ...]}} in call order: a
    set-up opens a solver call of its atom (a stepTo may run more than one
    substep, so more than one call), its passes follow."""
    out, open_, seq = {}, {}, {}
    for r in rows:
        if r[0] == "WS":
            open_[(r[1], r[2], r[3], r[4])] = r
        elif r[0] == "WE":
            ws = open_.pop((r[1], r[2], r[3], r[4]), None)
            if ws is None:
                continue
            if r[3] == "s":
                k = seq[(r[1], r[2])] = seq.get((r[1], r[2]), -1) + 1
                out[(r[1], r[2], k)] = {"s": (ws, r), "p": []}
            else:
                k = seq.get((r[1], r[2]))
                if k is not None:
                    out[(r[1], r[2], k)]["p"].append((ws, r))
    return out


def fl(ws, off):
    i = (off - 0x18) // 4
    return ws[8][i:i + 3]


def body_txt(b):
    com, q, v, w, im, inv = b
    return " ".join(str(x) for x in com + q + v + w + [im] + inv)


def emit_calls(path):
    for (n, atom, q), c in sorted(calls(load(path)).items()):
        ws, _ = c["s"]
        dt = ws[5][0]
        geo = fl(ws, 0x58) + fl(ws, 0x68) + fl(ws, 0x78) + fl(ws, 0x88) + fl(ws, 0xc8)
        print("C", f"{n}:{atom}:{q}", dt, body_txt(ws[6]), body_txt(ws[7]), " ".join(map(str, geo)), len(c["p"]))
        for pws, _ in c["p"]:
            tau, damp = pws[5][2], pws[5][3]
            print("P", tau, damp, " ".join(map(str, pws[6] + pws[7])))


def vel_entry(ws, stage):
    if stage == "s":
        a, b = ws[6], ws[7]
        return a[2] + a[3] + b[2] + b[3]
    return ws[6] + ws[7]


def cmp(path, ours_path):
    cs = calls(load(path))
    ours = {}
    for line in open(ours_path):
        t = line.split()
        if t and t[0] == "O":
            ours[(t[1], int(t[2]))] = [float(x) for x in t[3:15]]
    names = ["dvA", "dwA", "dvB", "dwB"]
    tot = {}
    worst = []
    for (n, atom, q), c in sorted(cs.items()):
        stages = [("s", c["s"])] + [("p%d" % i, p) for i, p in enumerate(c["p"])]
        for k, (name, (ws, we)) in enumerate(stages):
            o = ours.get((f"{n}:{atom}:{q}", k))
            if o is None:
                continue
            e = vel_entry(ws, "s" if k == 0 else "p")
            x = we[5] + we[6]
            for j, nm in enumerate(names):
                live = [x[3 * j + i] - e[3 * j + i] for i in range(3)]
                our = [o[3 * j + i] - e[3 * j + i] for i in range(3)]
                d = math.dist(live, our)
                m = math.hypot(*live)
                key = (name, nm)
                s = tot.setdefault(key, [0, 0.0, 0.0])
                s[0] += 1
                s[1] += d
                s[2] += m
                worst.append((d, n, atom, name, nm, live, our))
    print("stage body  calls  mean|live-ours|  mean|live|")
    for (name, nm), (cnt, d, m) in sorted(tot.items()):
        print(f"{name:5} {nm:4} {cnt:6} {d / cnt:12.5f} {m / cnt:12.5f}")
    worst.sort(key=lambda w: -w[0])
    for w in worst[:12]:
        print("worst", f"{w[0]:.4f}", w[1], w[2], w[3], w[4], [round(v, 4) for v in w[5]], [round(v, 4) for v in w[6]])


def track(path):
    rows = load(path)
    steer = {}
    for r in rows:
        if r[0] == "ST":
            steer[r[2]] = r[3]
            print("ST", r[1], r[2], r[3])
    for (n, atom, q), c in sorted(calls(rows).items()):
        ws, we = c["s"]
        print("W", n, atom, " ".join(f"{x:.4f}" for x in fl(ws, 0x78)), ws[8][(0x38 - 0x18) // 4])


if __name__ == "__main__":
    if sys.argv[1] == "calls":
        emit_calls(sys.argv[2])
    elif sys.argv[1] == "cmp":
        cmp(sys.argv[2], sys.argv[3])
    else:
        track(sys.argv[2])
