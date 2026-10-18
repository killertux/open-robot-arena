#!/usr/bin/env python3
"""Deterministic-ish driving test on the ORIGINAL: one robot driven by hand
inside a match, the opponent inert, sampled at ~10+ Hz.

Needs `tools/capture/original_session.sh start` and the exhibition set up with
two robots (routes/exhibition_box_scout_alarm.steps).  It:
  1. selects Combat Arena, starts the match, skips the intro, waits for the fight;
  2. in-game Python neuters the AI brains (Think no-op, no tactics), so robot 0
     (Scout) takes only our commands and robot 1 (ALARM) sits still at its spawn;
  3. samples location/heading/speed/hitpoints plus the *commanded* throttle and
     turn (SuperAI.set_throttle/set_turn_throttle, so the command edges are in
     the trace) while: turn 100 in place ~3 s, coast ~1.5 s, throttle 100 ~2.2 s, stop ~3 s;
  4. ends the match and goes back to the setup screen.

Output: local/oracle/original/drive-<bot>-box__live__<seq>.tsv and a .json with
the schedule used by `fidelity.py --gameplay`.
"""
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
PY = os.environ.get("RA2_FRIDA_PYTHON", os.path.expanduser("~/tools/frida-venv/bin/python3"))
LOCAL = os.path.join(os.path.dirname(os.path.dirname(HERE)), "local")
ORIG = os.path.join(LOCAL, "oracle", "original")
EXPR = ("repr([(p, plus.getLocation(p), plus.getHeading(p), plus.getSpeed(p), "
        "plus.getHitpoints(p, 0), plus.getHealth(p, 0), "
        "__import__('AI').running_ai[p].set_throttle, "
        "__import__('AI').running_ai[p].set_turn_throttle) for p in plus.getPlayers()])")

NEUTER = """
import AI, plus
def _nop(self):
    pass
for a in AI.running_ai:
    a.__class__.Think = _nop
    a.tactics = []
    a.Throttle(0)
    a.Turn(0)
out.write(repr(plus.getTimeElapsed()))
"""


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=240, **kw)


def py(code):
    return run([PY, os.path.join(HERE, "ra2_python.py"), "-c", code]).stdout


def drive(steps):
    return run([PY, os.path.join(HERE, "ra2_drive.py")] + steps)


def start_match(arena_y="188", tries=3):
    """Arena tab -> pick the row -> Start Game -> wait out the loading screen ->
    Skip Intro -> wait for the fight.  Loading is 5-20 s; running Python in the
    game while it loads can leave a black screen, so wait blind, then verify."""
    for _ in range(tries):
        drive(["click", "176", "177", "wait", "1.5", "click", "322", arena_y, "wait", "1.5", "hide",
               "wait", "1", "click", "936", "647", "hide", "wait", "14",
               "click", "512", "735", "hide", "wait", "8"])
        if "(0, 1)" in py("import plus; out.write(repr(plus.getPlayers()))"):
            return
        time.sleep(10)
    sys.exit("start_match: the match did not start")


def main():
    start_match(os.environ.get("ARENA_ROW_Y", "188"))
    print("fight clock:", py("import plus; out.write(repr(plus.getTimeElapsed()))"))
    print("neuter:", py(NEUTER))
    sampler = subprocess.Popen(
        [PY, os.path.join(HERE, "ra2_python.py"), "--sample", EXPR, "--every", "3",
         "--seconds", "20"], stdout=subprocess.PIPE, text=True)
    time.sleep(3.5)
    plan = [("turn", "import AI\nAI.running_ai[0].Throttle(0)\nAI.running_ai[0].Turn(100)", 3.0),
            ("coast", "import AI\nAI.running_ai[0].Throttle(0)\nAI.running_ai[0].Turn(0)", 1.5),
            ("straight", "import AI\nAI.running_ai[0].Throttle(100)\nAI.running_ai[0].Turn(0)", 2.2),
            ("stop", "import AI\nAI.running_ai[0].Throttle(0)\nAI.running_ai[0].Turn(0)", 3.0)]
    for name, code, hold in plan:
        t = time.time()
        print(name, py(code))
        time.sleep(max(0, hold - (time.time() - t)))
    text = sampler.communicate()[0]
    n = 0
    while os.path.exists(os.path.join(ORIG, f"drive-scout-box__live__{n:03d}.tsv")):
        n += 1
    path = os.path.join(ORIG, f"drive-scout-box__live__{n:03d}.tsv")
    with open(path, "w") as f:
        f.write("time_elapsed\t[(player, location, heading, speed, hitpoints, health, cmd_throttle, cmd_turn)]\n")
        f.write(text)
    json.dump({"bot": "AI/Team0/Bot0.bot", "arena": "box", "seconds": 9.7,
               "ours_script": "0-3:0,1;3-4.5:0,0;4.5-6.7:1,0;6.7-9.7:0,0",
               "note": "phases: turn in place ~3 s, coast ~1.5 s, full throttle ~2.2 s, stop ~3 s; the real edges are in columns cmd_throttle/cmd_turn"},
              open(path[:-4] + ".json", "w"), indent=1)
    print("wrote", path, len(text.splitlines()), "rows")
    py("import plus; plus.eliminatePlayer(1)")
    time.sleep(6)
    drive(["click", "717", "575", "hide", "wait", "3"])


if __name__ == "__main__":
    main()
