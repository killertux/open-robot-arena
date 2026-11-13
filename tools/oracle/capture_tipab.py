#!/usr/bin/env python3
"""Controlled tip experiments on the ORIGINAL (docs/29 section 12r).

Needs `tools/capture/original_session.sh start`.

  capture_tipab.py setup          main menu -> box Scout v ALARM, hazards off
                                  (HAZARDS_ON=1 leaves them on, docs/29 12bf),
                                  match started, both AIs neutered
  capture_tipab.py prepare        the same up to the setup screen (not started)
  capture_tipab.py restart        after a match: results -> the next match, AIs neutered
  capture_tipab.py bodies         every body the agent has seen (mass, centre)
  capture_tipab.py run grid.json  one trial per grid entry (frida/ra2_tipab.js):
      {"name": "...", "trials": [{"axis": [1,0,0], "deg": -100, "w": 1.0,
        "centre": [x,y,z], "v": [0,0,0], "steps": 240, "fire_py": 30}, ...]}
      (fire_py: the hammer is fired through ALARM's AI, Input("Fire", 0, 1),
      once that many substeps have run since the release; drive_py: [[substep,
      throttle, turn], ...] commands ALARM's AI the same way)
      ALARM's bodies are the rigid bodies within 1.6 m of its chassis (the
      chassis that weighs 232.1 kg). Writes OUT (default local/oracle/original)
      tipab-<name>__live__<seq>.jsonl (all rows, each trial's from its first
      `X` hold row to its `done` row) and .json (the grid, the bodies).

REST=<file>: the rest pose to start from ([[centre, quaternion w x y z], ...] per
body, as a `Y` row prints it), when the robot is not standing at rest.

RA2_FRIDA_HOST (default 127.0.0.1:27142) and RA2_CAPTURE_DISPLAY as the other
capture scripts.
"""
import json
import math
import os
import sys
import time

import frida

import capture_drive as cd


def qrot(q, v):
    """Rotate v by the quaternion q = [w, x, y, z]."""
    w, x, y, z = q
    px, py, pz = v
    # q * (0, v) * conj(q)
    aw, ax, ay, az = -x * px - y * py - z * pz, w * px + y * pz - z * py, w * py - x * pz + z * px, w * pz + x * py - y * px
    return (-aw * x + ax * w - ay * z + az * y, -aw * y + ay * w - az * x + ax * z, -aw * z + az * w - ax * y + ay * x)

ALARM = int(os.environ.get("ALARM_AI", "1"))  # the robot's index in AI.running_ai
# The robot under test, by its chassis mass (default ALARM's), and its weapon
# trigger (ALARM's Poker: "Fire"; the Scout's Flipper: "Flip").
CHASSIS = float(os.environ.get("CHASSIS", "232.116"))
TRIGGER = os.environ.get("TRIGGER", "Fire")
# A second robot put in place with the first in a trial's "extra" (docs/29 12bd), by its chassis mass
# (default ALARM's); the robot commanded through the AI (ALARM_AI, TRIGGER) is whichever the run sets.
CHASSIS2 = float(os.environ.get("CHASSIS2", "232.116"))
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
AGENT = os.path.join(cd.HERE, "frida", "ra2_tipab.js")


def setup(start=True):
    cd.drive(["-f", os.path.join(cd.HERE, "routes", os.environ.get("ROUTE", "exhibition_box_scout_alarm.steps"))])
    shot = os.path.join(os.environ.get("SHOT_DIR", "/tmp"), "tipab-setup.png")
    cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", os.environ.get("ARENA_ROW_Y", "188"), "wait", "1", "hide", "shot", shot])
    sys.path.insert(0, cd.HERE)
    from compare import read_png
    w, h, rgb = read_png(shot)
    v = [rgb[3 * (y * w + x) + c] for y in range(484, 493) for x in range(291, 300) for c in range(3)]
    if sum(v) / len(v) > 130 and os.environ.get("HAZARDS_ON"):
        print("hazards box checked: kept (HAZARDS_ON=1)")
    elif sum(v) / len(v) > 130:
        print("hazards box checked: clearing")
        cd.drive(["click", "295", "488", "wait", "1", "hide"])
    if not start:
        return
    cd.drive(["click", "936", "647", "hide", "wait", "14", "click", "512", "735", "hide", "wait", "1"])
    print("neuter:", cd.py(cd.NEUTER))


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


def alarm_bodies(ag, mass=None):
    mass = CHASSIS if mass is None else mass
    ch = [c for c in ag.api.chassis() if abs(c[1] - mass) < 0.05]
    if not ch:
        sys.exit("no ALARM chassis seen: %r" % ag.api.chassis())
    c0 = ch[0]
    # A body that touches nothing (the hammer's rod) is not known from the
    # contacts: look for rigid bodies (vtable 0x6abe04) around the chassis.
    a = int(c0[0], 16)
    cands = ag.api.bodies() + ag.api.scan(hex(a - 0x100000), hex(a + 0x100000))
    near, seen = [], {c0[0]}
    for b in cands:
        if b[0] not in seen and b[1] > 0 and math.dist(b[2], c0[2]) < 1.6:
            seen.add(b[0])
            near.append(b)
    return [c0] + near


def main():
    cmd = sys.argv[1]
    if cmd in ("setup", "prepare"):
        setup(cmd == "setup")
        return
    if cmd == "restart":
        # The match is over (a box match lasts 3 minutes): results -> setup
        # screen -> start again, the AIs neutered.
        cd.drive(["click", "717", "575", "hide", "wait", "4"])
        cd.drive(["click", "936", "647", "hide", "wait", "14", "click", "512", "735", "hide", "wait", "1"])
        print("neuter:", cd.py(cd.NEUTER))
        return
    ag = Agent()
    time.sleep(2.0)
    print("immobility limit was", ag.api.no_immobility())
    if cmd == "bodies":
        for c in ag.api.chassis():
            print("chassis", c)
        for b in ag.api.bodies():
            print("body", b)
        ag.close()
        return
    grid = json.load(open(sys.argv[2]))
    out_dir = os.environ.get("OUT", cd.ORIG)
    bodies = alarm_bodies(ag)
    print("ALARM bodies:", [(b[0], round(b[1], 3)) for b in bodies])
    if len(bodies) != 6:
        print("warning: expected 6 bodies (chassis, four wheels, rod)")
    watched = [b[0] for b in bodies]
    bodies2 = None
    if any("extra" in t for t in grid["trials"]):
        bodies2 = alarm_bodies(ag, CHASSIS2)
        print("second robot bodies:", [(b[0], round(b[1], 3)) for b in bodies2])
        # Rest poses at the start (the agent keeps the first plan's): centre and quaternion [w x y z].
        c_main, c_two = bodies[0][2], bodies2[0][2]
        q_two = ag.api.peek(hex(int(bodies2[0][0], 16) + 0x40), 16)[:4]
        print("second robot rest", c_two, q_two)
        watched += [b[0] for b in bodies2]
    ag.api.watch(watched)
    if os.environ.get("HINGE"):
        ag.api.hinge(True)  # HN rows: every hinge atom set-up of the watched bodies (docs/29 12bi)
    ptrs = [b[0] for b in bodies]
    first = int(os.environ.get("FIRST", "0"))
    last = int(os.environ.get("LAST", str(len(grid["trials"]))))
    for i, t in enumerate(grid["trials"]):
        if not first <= i < last:
            continue
        plan = {"bodies": ptrs, "hold": t.get("hold", 30), "lift": 1.0, "axis": t["axis"],
                "angle": math.radians(t["deg"]), "w": t["w"], "v": t.get("v", [0, 0, 0]),
                "centre": t.get("centre"), "dcentre": t.get("dcentre"), "steps": t.get("steps", 240), "fire": t.get("fire", -1),
                "piston": t.get("piston"), "wspin": t.get("wspin")}
        if i == first and os.environ.get("REST"):
            plan["rest"] = json.load(open(os.environ["REST"]))
        if "extra" in t:
            ex = t["extra"]
            az = math.radians(ex["az"])
            tx, tz = c_main[0] + ex["dist"] * math.cos(az), c_main[2] + ex["dist"] * math.sin(az)
            # forward (chassis +z) of the second robot at rest, turned about its chassis y axis to face the first
            f0 = qrot(q_two, (0, 0, 1))
            psi = math.atan2(c_main[0] - tx, c_main[2] - tz) - math.atan2(f0[0], f0[2])
            plan["extra"] = [{"bodies": [b[0] for b in bodies2], "axis": [0, 1, 0], "angle": psi, "w": 0,
                              "centre": [tx, c_two[1] + 0.03, tz]}]
            ag.rows.append(["EXTRA", i, tx, tz, math.degrees(psi), f0])
        ag.rows.append(["TRIAL", i, t])
        ag.api.arm(plan)
        t0 = time.time()
        fired = t.get("fire_py") is None
        drives = list(t.get("drive_py", []))  # [[substep, throttle, turn], ...]
        while ag.api.busy() and time.time() - t0 < 60:
            if drives and ag.api.step() >= drives[0][0]:
                st, th, tu = drives.pop(0)
                ag.rows.append(["DRIVEPY", i, ag.api.step(), th, tu])
                cd.py("import AI\nAI.running_ai[%d].Throttle(%s)\nAI.running_ai[%d].Turn(%s)" % (ALARM, th, ALARM, tu))
                continue
            if not fired and ag.api.step() >= t["fire_py"]:
                # The hammer fired through the robot's controls, as its AI
                # does (Poker: Input("Fire", 0, 1)); when it fires shows in
                # the rod's B rows.
                ag.rows.append(["FIREPY", i, ag.api.step()])
                print(" fire:", cd.py("import AI\nAI.running_ai[%d].Input('%s', 0, 1)\n"
                                      "out.write(repr(AI.running_ai[%d].name))" % (ALARM, TRIGGER, ALARM)))
                fired = True
            time.sleep(0.05 if not fired else 0.3)
        ag.api.flush()
        time.sleep(0.4)
        print("trial", i, t.get("label", ""), "done", round(time.time() - t0, 1), "s", flush=True)
    ag.close()
    name = grid["name"]
    n = 0
    while os.path.exists(os.path.join(out_dir, f"tipab-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"tipab-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in ag.rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    json.dump({"grid": grid, "first": first, "last": last, "bodies": bodies, "rows": len(ag.rows)}, open(base + ".json", "w"), indent=1)
    print("wrote", base, len(ag.rows), "rows")


if __name__ == "__main__":
    main()
