#!/usr/bin/env python3
"""The game's drive model (MagicAction 0x4856f0) measured live in the ORIGINAL.

Needs `tools/capture/original_session.sh start` and the exhibition set up as for
capture_drive.py (routes/exhibition_box_scout_alarm.steps).  Neuters the AI,
then drives robot PLAYER (env, default 0 = Scout; 1 = ALARM) through the drive
test (turn 3 s, coast 1.5 s, straight 3 s, stop 3 s) while
frida/ra2_magic.js records the drive model's per-wheel records and the chassis
body every Havok step.  Writes
  local/oracle/original/magic-<bot>__live__<seq>.jsonl     raw rows
  local/oracle/original/magic-<bot>__live__<seq>.speed.tsv plus.* trace
  local/oracle/original/magic-<bot>__live__<seq>.json      marks
Set SKIP_START=1 when the match is already running.
"""
import json
import os
import subprocess
import sys
import time

import frida

import capture_drive as cd

AGENT = os.path.join(cd.HERE, "frida", "ra2_magic.js")
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


STATE = ("import plus\nout.write(repr((plus.getLocation(%d), plus.getHeading(%d), "
         "[plus.getLocation(p) for p in plus.getPlayers() if p != %d])))")
HALF = float(os.environ.get("ARENA_HALF", "11.0"))


def state(player):
    loc, head, others = eval(cd.py(STATE % (player, player, player)))
    return loc, head, others


def free_run(x, z, d):
    """Distance from (x, z) along unit direction d to the arena's square walls."""
    best = 1e9
    for p, v in ((x, d[0]), (z, d[1])):
        if abs(v) > 1e-6:
            best = min(best, ((HALF if v > 0 else -HALF) - p) / v)
    return best


def aim(player):
    """Point the robot at the longest free straight line, with the game's own
    AI steering (`AimToHeading(GetHeadingTo(target))`) installed as its Think
    for a few seconds, then neuter it again."""
    import math
    (x, _, z), _, others = state(player)
    best, target = -1, (0.0, 0.0)
    for k in range(72):
        a = k * math.tau / 72
        d = (math.sin(a), math.cos(a))
        if any(((ox - x) * d[0] + (oz - z) * d[1]) > -1.0 and
               abs((ox - x) * d[1] - (oz - z) * d[0]) < 2.0 for ox, _, oz in others):
            continue
        r = free_run(x, z, d)
        if r > best:
            best, target = r, (x + d[0] * r, z + d[1] * r)
    print(f"aim: from ({x:.1f}, {z:.1f}) to ({target[0]:.1f}, {target[1]:.1f}), free run {best:.1f} m")
    out = cd.py(f"""import AI
def _aim(self):
    self.AimToHeading(self.GetHeadingTo(({target[0]}, {y_of(player)}, {target[1]}), 0), 0)
import new
_a = AI.running_ai[{player}]
_a.Think = new.instancemethod(_aim, _a, _a.__class__)
out.write('ok')""")
    print("aim install:", out.strip()[:200])
    time.sleep(float(os.environ.get("AIM_SECONDS", "4")))
    cd.py(f"""import AI
def _nop(self):
    pass
import new
_a = AI.running_ai[{player}]
_a.Think = new.instancemethod(_nop, _a, _a.__class__)
AI.running_ai[{player}].Turn(0)
AI.running_ai[{player}].Throttle(0)""")
    time.sleep(1.0)


def y_of(player):
    return state(player)[0][1]


def main():
    player = int(os.environ.get("PLAYER", "0"))
    name = os.environ.get("BOT", ["scout", "alarm"][player] if player < 2 else f"p{player}")
    if os.environ.get("SKIP_START") != "1":
        cd.start_match(os.environ.get("ARENA_ROW_Y", "188"))
    print("neuter:", cd.py(cd.NEUTER))
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    rows = []

    def on_msg(m, d):
        if m.get("type") == "send" and m["payload"].get("ev") == "batch":
            rows.extend(m["payload"]["rows"])
        elif m.get("type") != "send":
            print("agent error", m, file=sys.stderr)

    sc = sess.create_script(open(AGENT).read())
    sc.on("message", on_msg)
    sc.load()
    n = 0
    while os.path.exists(os.path.join(cd.ORIG, f"magic-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(cd.ORIG, f"magic-{name}__live__{n:03d}")
    sampler = subprocess.Popen(
        [cd.PY, os.path.join(cd.HERE, "ra2_python.py"), "--sample", cd.EXPR, "--every", "2",
         "--seconds", os.environ.get("SAMPLE_SECONDS", "40")], stdout=subprocess.PIPE, text=True)
    time.sleep(2.5)
    t0 = time.time()
    ai = f"import AI\nAI.running_ai[{player}]"
    plan = [("turn", f"{ai}.Throttle(0)\nAI.running_ai[{player}].Turn(100)", 3.0),
            ("coast", f"{ai}.Throttle(0)\nAI.running_ai[{player}].Turn(0)", 1.5),
            ("straight", f"{ai}.Throttle(100)\nAI.running_ai[{player}].Turn(0)", float(os.environ.get("STRAIGHT", "2.2"))),
            ("stop", f"{ai}.Throttle(0)\nAI.running_ai[{player}].Turn(0)", 3.0)]
    marks = []
    for mark, code, hold in plan:
        if mark == "straight" and os.environ.get("AIM", "1") == "1":
            aim(player)
        t = time.time()
        cd.py(code)
        marks.append((mark, t - t0))
        time.sleep(max(0, hold - (time.time() - t)))
    text = sampler.communicate()[0]
    cd.py("import plus; plus.eliminatePlayer(1 - %d)" % player if player < 2 else "pass")
    sc.exports_sync.flush()
    time.sleep(0.5)
    sc.unload()
    sess.detach()
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    with open(base + ".speed.tsv", "w") as f:
        f.write("time_elapsed\t[(player, location, heading, speed, hitpoints, health, cmd_throttle, cmd_turn)]\n")
        f.write(text)
    json.dump({"player": player, "marks": marks, "rows": len(rows)}, open(base + ".json", "w"), indent=1)
    print("wrote", base, len(rows), "rows", marks)
    time.sleep(6)
    cd.drive(["click", "717", "575", "hide", "wait", "3"])


if __name__ == "__main__":
    main()
