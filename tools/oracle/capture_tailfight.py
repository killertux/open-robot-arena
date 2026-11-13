#!/usr/bin/env python3
"""A fight from a placed state on the ORIGINAL, both shipped AIs running (docs/29 section 12bg).

Needs `tools/capture/original_session.sh start` (and the live lock, docs/29 rules). The Scout is stood on its tail at a place of the
arena (pitch -90 about its chassis x axis, as the tipab grids), ALARM is put `dist` m from it in the direction `az` (0 = +x, 90 = +z, from the
Scout's placed centre) facing it, both released together (frida/ra2_tipab.js), and from the release both AIs are switched on
(until then each brain's `Think` is an empty function on the instance and its tactics are put aside: the soft neuter below, undone by `AI_ON`). The agent writes the `B` rows of every body of both
robots, the wheel records (`W`), the Scout's fork fires (`FM`) and ALARM's piston fires (`BF`); the brains' own `Input` calls and
the tactic of every Think are logged by wrapping the brains' methods in the game's Python (`AI._log`) and read back after the trial.

  capture_tailfight.py grid.json         one fresh match per trial: setup, soft neuter, place, AIs on, `secs` of fight, End Match
  grid: {"name": "...", "arena_row_y": "296", "trials": [{"label": "...", "dx": -3.2, "dz": 3.2, "az": -45, "dist": 2.5, "secs": 12}, ...]}
  FIRST / LAST   run only the trials FIRST <= i < LAST (the first one does the route from the main menu)
  HAZARDS_ON=1   the arena's hazards left on (the default here)
  OUT            the directory of the captures (default local/oracle/original)

dx, dz: the Scout's centre from its rest centre (the grids' `dcentre`, y 0.3598 stands it on its tail). Output
`tailfight-<name>__live__<seq>.jsonl` (all rows, each trial's from its `TRIAL` row) and `.json` (grid, bodies, the brains' logs).
RA2_FRIDA_HOST (default 127.0.0.1:27142) and RA2_CAPTURE_DISPLAY as the other capture scripts.
"""
import json
import math
import os
import sys
import time

import capture_drive as cd
import capture_tipab as ct

SOFT_NEUTER = """
import AI, plus
def _nop():
    pass
for a in AI.running_ai:
    a._saved = a.tactics
    a.tactics = []
    a.Think = _nop
    a.Throttle(0)
    a.Turn(0)
out.write(repr(plus.getTimeElapsed()))
"""

# The brains' Input / Evaluate wrapped so that every command and every decision is logged with the game clock.
LOGGER = """
import AI, plus
AI._log = []
def _wrap(a):
    inp = a.Input
    ev = a.Evaluate
    def Input(name, index, value, inp=inp, a=a):
        AI._log.append((plus.getTimeElapsed(), a.GetID(), 'I', name, index, value))
        return inp(name, index, value)
    def Evaluate(ev=ev, a=a):
        r = ev()
        AI._log.append((plus.getTimeElapsed(), a.GetID(), 'E', a.tactics[0].name, a.tactics[0].priority, a.IsUpsideDown()))
        return r
    a.Input = Input
    a.Evaluate = Evaluate
for a in AI.running_ai:
    _wrap(a)
out.write(repr(plus.getTimeElapsed()))
"""

AI_ON = """
import AI, plus
for a in AI.running_ai:
    del a.Think
    a.tactics = a._saved
    a.countdownToEvaluation = 0
out.write(repr(plus.getTimeElapsed()))
"""

DUMP = """
import AI
out.write(repr(AI._log))
"""

END_MATCH = "import plus\nplus.eliminatePlayer(1)"


def start_match(first, arena_row_y):
    if first:
        os.environ["ARENA_ROW_Y"] = arena_row_y
        cd.drive(["-f", os.path.join(cd.HERE, "routes", os.environ.get("ROUTE", "exhibition_octagon_scout_alarm.steps"))])
        shot = os.path.join(os.environ.get("SHOT_DIR", "/tmp"), "tailfight-setup.png")
        cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", arena_row_y, "wait", "1", "hide", "shot", shot])
        sys.path.insert(0, cd.HERE)
        from compare import read_png
        w, h, rgb = read_png(shot)
        v = [rgb[3 * (y * w + x) + c] for y in range(484, 493) for x in range(291, 300) for c in range(3)]
        if sum(v) / len(v) > 130 and not os.environ.get("HAZARDS_OFF"):
            print("hazards box checked: kept")
        elif sum(v) / len(v) > 130:
            print("hazards box checked: clearing")
            cd.drive(["click", "295", "488", "wait", "1", "hide"])
    cd.drive(["click", "936", "647", "hide", "wait", "14", "click", "512", "735", "hide", "wait", "1"])
    print("soft neuter:", cd.py(SOFT_NEUTER))
    print("logger:", cd.py(LOGGER))


def end_match():
    cd.py(END_MATCH)
    time.sleep(6)
    cd.drive(["click", "717", "575", "hide", "wait", "4"])


def main():
    grid = json.load(open(sys.argv[1]))
    arena_row_y = grid.get("arena_row_y", "296")
    out_dir = os.environ.get("OUT", cd.ORIG)
    first = int(os.environ.get("FIRST", "0"))
    last = int(os.environ.get("LAST", str(len(grid["trials"]))))
    rows_all, logs, meta = [], {}, {}
    for i, t in enumerate(grid["trials"]):
        if not first <= i < last:
            continue
        start_match(i == first, arena_row_y)
        ag = ct.Agent()
        time.sleep(2.0)
        print("immobility limit was", ag.api.no_immobility())
        bodies = ct.alarm_bodies(ag)
        bodies2 = ct.alarm_bodies(ag, ct.CHASSIS2)
        print("Scout bodies:", [(b[0], round(b[1], 3)) for b in bodies], "ALARM bodies:", [(b[0], round(b[1], 3)) for b in bodies2])
        c_main, c_two = bodies[0][2], bodies2[0][2]
        q_two = ag.api.peek(hex(int(bodies2[0][0], 16) + 0x40), 16)[:4]
        ag.api.watch([b[0] for b in bodies] + [b[0] for b in bodies2])
        ag.api.wheels(True)
        # ALARM `dist` from the Scout's placed centre in the direction `az`, facing it.
        sx, sz = c_main[0] + t["dx"], c_main[2] + t["dz"]
        az = math.radians(t["az"])
        tx, tz = sx + t["dist"] * math.cos(az), sz + t["dist"] * math.sin(az)
        f0 = ct.qrot(q_two, (0, 0, 1))
        psi = math.atan2(sx - tx, sz - tz) - math.atan2(f0[0], f0[2])
        plan = {"bodies": [b[0] for b in bodies], "hold": 30, "lift": 1.0, "axis": [1, 0, 0], "angle": math.radians(-90), "w": 0,
                "v": [0, 0, 0], "dcentre": [t["dx"], 0.3598, t["dz"]], "steps": int(t.get("secs", 12) * 60), "fire": -1}
        plan["extra"] = [{"bodies": [b[0] for b in bodies2], "axis": [0, 1, 0], "angle": psi, "w": 0,
                          "centre": [tx, c_two[1] + 0.03, tz]}]
        ag.rows.append(["TRIAL", i, t])
        ag.rows.append(["EXTRA", i, tx, tz, math.degrees(psi), f0])
        ag.api.arm(plan)
        t0 = time.time()
        on = None
        while ag.api.busy() and time.time() - t0 < 120:
            if on is None and ag.api.step() >= int(t.get("ai_at", 0)):
                on = cd.py(AI_ON)
                ag.rows.append(["AION", i, ag.api.step(), on])
                continue
            time.sleep(0.05)
        ag.api.flush()
        time.sleep(0.4)
        log = cd.py(DUMP)
        ag.close()
        end_match()
        rows_all.extend(ag.rows)
        logs[i] = log
        meta[i] = {"bodies": bodies, "bodies2": bodies2, "ai_on_clock": on}
        print("trial", i, t.get("label", ""), "done", round(time.time() - t0, 1), "s", flush=True)
        # Keep what we have after every trial.
        name = grid["name"]
        n = 0
        while os.path.exists(os.path.join(out_dir, f"tailfight-{name}__live__{n:03d}.jsonl")):
            n += 1
        base = os.path.join(out_dir, f"tailfight-{name}__live__{n:03d}")
        with open(base + ".jsonl", "w") as f:
            for r in ag.rows:
                f.write(json.dumps(r, separators=(",", ":")) + "\n")
        json.dump({"grid": grid, "trial": i, "meta": meta[i], "brains": logs[i]}, open(base + ".json", "w"), indent=1)
        print("wrote", base, len(ag.rows), "rows")


if __name__ == "__main__":
    main()
