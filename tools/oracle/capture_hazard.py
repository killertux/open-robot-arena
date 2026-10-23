#!/usr/bin/env python3
"""The arenas' hinge hazards in the ORIGINAL (docs/29 section 12z): the
Octagon's hellraisers and hammers, the Compressor's smash arms, the Half
Circle's hammer, the Parking Lot's trap doors.

Needs `tools/capture/original_session.sh start` and the game at the main menu.
Sets up the exhibition (routes/exhibition_box_scout_alarm.steps: the prefix's
own Scout v ALARM, hazards as the setup screen has them), and for every
`name:row_y:seconds[:actions]` starts a match on that arena row (row y = 170 +
18 · row: 2 compressor, 4 halfcircle, 6 parkinglot, 7 octagon), neuters both
AIs, loads frida/ra2_hazard.js (and frida/ra2_twobody.js for placements), runs
the actions in order and ends the match. Actions, `;;`-separated:

  t=<python>            at wall time t (s, from the neuter) run the python
                        (`;` separates lines) through ra2_python.py
  place=<x>,<z>[,<steps>]   a ra2_twobody.js placement: both robots held 1 m
                        above their rest poses for 30 substeps, then ALARM's
                        chassis put at (x, z) at its rest height and heading,
                        the Scout 4 m in front of it, and the world left alone
                        for `steps` substeps (default 360)

    OUT=<dir> capture_hazard.py "octagon:296:40:place=-4.75,-0.26;;place=3.5,-0.26;;24=import Arenas;Arenas.currentArena.Hammer1.Smash()"

Writes OUT/hazard-<name>__live__<seq>.jsonl: ra2_hazard.js's rows, the
placement agent's rows tagged `["TB", row]`, `PY`/`TRIAL` markers with the
agent's stepTo count.
"""
import json
import os
import sys
import time

import capture_drive as cd
import capture_twobody as ct

AGENT = os.path.join(cd.HERE, "frida", "ra2_hazard.js")


def one(spec, out_dir):
    parts = spec.split(":", 3)
    name, row_y, secs = parts[0], parts[1], float(parts[2])
    actions = []
    if len(parts) > 3:
        for a in parts[3].split(";;"):
            k, v = a.split("=", 1)
            actions.append((k, v))
    cd.start_match(row_y)
    print("neuter:", cd.py(cd.NEUTER), flush=True)
    t0 = time.time()
    ag = ct.Agent()
    rows = []

    def on_msg(m, d):
        if m.get("type") == "send" and m["payload"].get("ev") == "batch":
            rows.extend(m["payload"]["rows"])
        elif m.get("type") != "send":
            print("agent error", m, file=sys.stderr)

    sc = ag.sess.create_script(open(AGENT).read())
    sc.on("message", on_msg)
    sc.load()
    api = sc.exports_sync
    print("immobility limit was", ag.api.no_immobility(), flush=True)
    rows.append(["NEUTER", api.flush()])
    bodies = None
    for k, v in actions:
        if k == "place":
            if bodies is None:
                time.sleep(1.5)
                bs = ct.robot_bodies(ag, ct.SCOUT)
                ba = ct.robot_bodies(ag, ct.ALARM)
                ptrs = [b[0] for b in bs] + [b[0] for b in ba]
                ag.api.learn_all(ptrs)
                rest = [[s[0], s[1]] for s in ag.api.states(ptrs)]
                bodies = (bs, ba, ptrs, rest)
                rows.append(["REST", rest, ptrs, len(bs)])
            bs, ba, ptrs, rest = bodies
            w = [float(x) for x in v.split(",")]
            trial = {"alarm_at": [w[0], w[1]], "face": "front", "gap": 4.0, "speed": 0.0,
                     "steps": int(w[2]) if len(w) > 2 else 360}
            place = ct.placement(trial, rest[:len(bs)], rest[len(bs):])
            plan = {"bodies": ptrs, "hold": 30, "lift": 1.0, "rest": rest, "place": place,
                    "steps": trial["steps"]}
            rows.append(["TRIAL", api.flush(), trial])
            ag.api.arm(plan)
            ta = time.time()
            while ag.api.busy() and time.time() - ta < 90:
                time.sleep(0.2)
            rows.append(["TRIALEND", api.flush()])
            print("placement", v, "done", round(time.time() - ta, 1), "s", flush=True)
        else:
            t = float(k)
            while time.time() - t0 < t:
                time.sleep(0.1)
            rows.append(["PY", api.flush(), v])
            print("py", t, cd.py(v.replace(";", "\n") + "\nout.write('ok')"), flush=True)
    while time.time() - t0 < secs:
        time.sleep(0.5)
    api.flush()
    ag.api.flush()
    time.sleep(0.8)
    sc.unload()
    ag.close()
    rows.extend([["TB"] + r for r in ag.rows])
    n = 0
    while os.path.exists(os.path.join(out_dir, f"hazard-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"hazard-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    print("wrote", base, len(rows), "rows", flush=True)
    cd.py("import plus; plus.eliminatePlayer(1)")
    time.sleep(6)
    cd.drive(["click", "717", "575", "hide", "wait", "3"])


def main():
    out_dir = os.environ.get("OUT", cd.ORIG)
    cd.drive(["-f", os.path.join(cd.HERE, "routes", "exhibition_box_scout_alarm.steps")])
    for spec in sys.argv[1:]:
        one(spec, out_dir)
    cd.drive(["click", "527", "745", "hide", "wait", "3"])


if __name__ == "__main__":
    main()
