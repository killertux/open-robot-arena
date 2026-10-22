#!/usr/bin/env python3
"""Controlled two-robot placements on the ORIGINAL (docs/29 section 12s).

Needs `tools/capture/original_session.sh start`, then `capture_tipab.py setup`
(box Scout v ALARM, hazards off, both AIs neutered).

  capture_twobody.py run grid.json   one trial per grid entry (frida/ra2_twobody.js)

Both robots are first held 1 m above their rest poses (where they stood when
the run began, or `REST=<file>`), then placed together and let go. ALARM's
chassis' centre goes to `alarm_at` (x, z; default 0, 0) at its rest height and
heading; the Scout is turned about the vertical and placed so that a point of
its fork (`tip`, the Scout's chassis frame, default the fork arm's lowest
front edge) sits `gap` m outside a point of ALARM (`target`):

  {"name": "...", "trials": [{"label": "...", "face": "side+"|"side-"|"front"|"back",
     "lat": 0.0, "gap": 0.05, "speed": 2.0, "steps": 180,
     "yaw": 0, "throttle": [scout, alarm], "fire_py": [[substep, who, trigger], ...]}]}

`face` picks ALARM's side (`side+` its +x, `side-` its -x, `front` +z, `back`
-z); `lat` moves the target along that face; the Scout approaches square to it
(or `yaw` degrees off square) at `speed` (every body of it at that
velocity), `dy` lifts it. `throttle` is set through the AIs before the hold
(so the motors have settled when the robots are let go); `fire_py` presses a
trigger through the AI (`Flip` for the Scout, `Fire` for ALARM) once that many
substeps have run since the release: when it lands shows in the `M`/`F` rows.
The neutered AIs still fire on their weapon zones, so the Scout's fork fires
whenever ALARM is over it: replay every `M`/`F` row, not the grid's fires.

Writes OUT (default local/oracle/original) twobody-<name>__live__<seq>.jsonl
(every row, a `TRIAL` row ahead of each trial's) and .json (the grid, the
bodies of each robot, the rest poses).
"""
import json
import math
import os
import sys
import time

import frida

import capture_drive as cd

HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
AGENT = os.path.join(cd.HERE, "frida", "ra2_twobody.js")
SCOUT, ALARM = 163.495, 232.116
# The Scout's fork arm's lowest front edge and ALARM's chassis' side, front
# and back faces, in each chassis' frame (centre of mass origin; our hull
# points, docs/29 section 12s).
TIP = [0.010, -0.239, 1.298]
FACES = {"side+": ([1, 0, 0], 0.88), "side-": ([-1, 0, 0], 0.87), "front": ([0, 0, 1], 0.856),
         "back": ([0, 0, -1], 0.684)}


def qmul(a, b):
    return [a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
            a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
            a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
            a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0]]


def qrot(q, v):
    p = qmul(qmul(q, [0] + list(v)), [q[0], -q[1], -q[2], -q[3]])
    return p[1:]


def add(a, b):
    return [x + y for x, y in zip(a, b)]


def sub(a, b):
    return [x - y for x, y in zip(a, b)]


def scale(a, s):
    return [x * s for x in a]


def flat(v):
    n = math.hypot(v[0], v[2])
    return [v[0] / n, 0.0, v[2] / n]


class Agent:
    def __init__(self):
        dev = frida.get_device_manager().add_remote_device(HOST)
        pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
        self.sess = dev.attach(pid)
        self.rows = []
        self.sc = self.sess.create_script(open(AGENT).read())
        self.sc.on("message", self.on_msg)
        self.sc.load()
        self.api = self.sc.exports_sync

    def on_msg(self, m, d):
        if m.get("type") == "send" and m["payload"].get("ev") == "batch":
            self.rows.extend(m["payload"]["rows"])
        elif m.get("type") != "send":
            print("agent error", m, file=sys.stderr)

    def close(self):
        self.api.flush()
        time.sleep(0.5)
        self.sc.unload()
        self.sess.detach()


def robot_bodies(ag, mass):
    ch = [c for c in ag.api.chassis() if abs(c[1] - mass) < 0.05]
    if not ch:
        sys.exit("no chassis of %.3f kg seen: %r" % (mass, ag.api.chassis()))
    c0 = ch[0]
    a = int(c0[0], 16)
    cands = ag.api.bodies() + ag.api.scan(hex(a - 0x100000), hex(a + 0x100000))
    near, seen = [], {c0[0]}
    for b in cands:
        if b[0] not in seen and b[1] > 0 and math.dist(b[2], c0[2]) < 1.6:
            seen.add(b[0])
            near.append(b)
    return [c0] + near


def placement(t, rest_s, rest_a):
    """Every body's [com, quat, vel, angvel] for a trial: ALARM's rest pose moved
    to `alarm_at`, the Scout turned and moved to face it."""
    ca, qa = rest_a[0]
    at = t.get("alarm_at", [0.0, 0.0])
    da = [at[0] - ca[0], 0.0, at[1] - ca[2]]
    va = t.get("alarm_v", [0, 0, 0])
    out_a = [[add(c, da), q, va, [0, 0, 0]] for c, q in rest_a]
    ca2 = add(ca, da)
    axis, half = FACES[t.get("face", "side+")]
    n = flat(qrot(qa, axis))
    lat_axis = [axis[2], 0, axis[0]] if axis[1] == 0 else [1, 0, 0]
    local = add(scale(axis, half), scale(lat_axis, t.get("lat", 0.0)))
    target = add(ca2, qrot(qa, local))
    # `yaw` (degrees) turns the Scout's approach about the vertical.
    y = math.radians(t.get("yaw", 0.0))
    n = [n[0] * math.cos(y) + n[2] * math.sin(y), 0.0, -n[0] * math.sin(y) + n[2] * math.cos(y)]
    # The Scout turned about the vertical to face -n.
    cs, qs = rest_s[0]
    fs = flat(qrot(qs, [0, 0, 1]))
    ang = math.atan2(-n[0], -n[2]) - math.atan2(fs[0], fs[2])
    Q = [math.cos(ang / 2), 0.0, math.sin(ang / 2), 0.0]
    tip = qrot(Q, qrot(qs, t.get("tip", TIP)))
    want = add(target, scale(n, t.get("gap", 0.05)))
    cs2 = [want[0] - tip[0], cs[1] + t.get("dy", 0.0), want[2] - tip[2]]
    vs = scale(n, -t.get("speed", 0.0))
    out_s = [[add(cs2, qrot(Q, sub(c, cs))), qmul(Q, q), vs, [0, 0, 0]] for c, q in rest_s]
    return out_s + out_a


def main():
    ag = Agent()
    time.sleep(2.0)
    print("immobility limit was", ag.api.no_immobility())
    grid = json.load(open(sys.argv[2]))
    out_dir = os.environ.get("OUT", cd.ORIG)
    bs = robot_bodies(ag, SCOUT)
    ba = robot_bodies(ag, ALARM)
    print("Scout bodies:", [(b[0], round(b[1], 3)) for b in bs])
    print("ALARM bodies:", [(b[0], round(b[1], 3)) for b in ba])
    ptrs = [b[0] for b in bs] + [b[0] for b in ba]
    ag.api.learn_all(ptrs)
    if os.environ.get("REST"):
        rest = json.load(open(os.environ["REST"]))
    else:
        rest = [[s[0], s[1]] for s in ag.api.states(ptrs)]
    rest_s, rest_a = rest[:len(bs)], rest[len(bs):]
    ag.rows.append(["REST", rest, ptrs])
    first = int(os.environ.get("FIRST", "0"))
    last = int(os.environ.get("LAST", str(len(grid["trials"]))))
    for i, t in enumerate(grid["trials"]):
        if not first <= i < last:
            continue
        th = t.get("throttle", [0, 0])
        cd.py("import AI\nAI.running_ai[0].Throttle(%s)\nAI.running_ai[0].Turn(0)\n"
              "AI.running_ai[1].Throttle(%s)\nAI.running_ai[1].Turn(0)" % (th[0], th[1]))
        place = placement(t, rest_s, rest_a)
        plan = {"bodies": ptrs, "hold": t.get("hold", 30), "lift": 1.0, "rest": rest,
                "place": place, "steps": t.get("steps", 180)}
        ag.rows.append(["TRIAL", i, t])
        ag.api.arm(plan)
        t0 = time.time()
        fires = sorted(t.get("fire_py", []))
        while ag.api.busy() and time.time() - t0 < 60:
            if fires and ag.api.step() >= fires[0][0]:
                st, who, trig = fires.pop(0)
                ag.rows.append(["FIREPY", i, ag.api.step(), who, trig])
                cd.py("import AI\nAI.running_ai[%d].Input('%s', 0, 1)" % (who, trig))
                continue
            time.sleep(0.01 if fires else 0.2)
        cd.py("import AI\nfor a in AI.running_ai:\n    a.Throttle(0)\n    a.Turn(0)")
        ag.api.flush()
        time.sleep(1.0)
        print("trial", i, t.get("label", ""), "done", round(time.time() - t0, 1), "s", flush=True)
    ag.close()
    name = grid["name"]
    n = 0
    while os.path.exists(os.path.join(out_dir, f"twobody-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"twobody-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in ag.rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    json.dump({"grid": grid, "first": first, "last": last, "scout": bs, "alarm": ba, "rest": rest,
               "rows": len(ag.rows)}, open(base + ".json", "w"), indent=1)
    print("wrote", base, len(ag.rows), "rows")


if __name__ == "__main__":
    if len(sys.argv) < 3 or sys.argv[1] != "run":
        sys.exit(__doc__)
    main()
