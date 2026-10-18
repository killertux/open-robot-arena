#!/usr/bin/env python3
"""Weapon/damage test on the ORIGINAL: one inert robot (Scout, AI neutered) is
attacked by ALARM's own AI (Poker brain, weapon component 13) in Combat Arena,
with the damage hook (ra2_damage.js) and a ~13 Hz sampler running.

Spinner/hammer robots (Team Z's Berserker, Team 2's BEAR...) would be better
attackers but the Select Computer Robot team drop-down does not open under our
Frida mouse injection, so only the default Red Zone robots can be picked.

Output (local/oracle/original/):
  weapon-alarm-vs-inert-scout__live__<n>.tsv   sampler rows, incl. per-component
                                               health of both robots
  weapon-alarm-vs-inert-scout__live__<n>.jsonl damage-sink log
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import capture_drive as cd  # noqa: E402

# Scout has 11 components, ALARM 16 (getHealth raises past the end).
EXPR = ("repr([(p, plus.getLocation(p), plus.getHeading(p), plus.getSpeed(p), "
        "plus.getHitpoints(p, 0), plus.getHealth(p, 0), "
        "[round(plus.getHealth(p, c), 3) for c in range([11, 16][p])]) for p in plus.getPlayers()])")


def main():
    seconds = float(os.environ.get("WEAPON_SECONDS", "40"))
    if not os.environ.get("ALREADY_STARTED"):
        cd.start_match()
    neuter = ("import AI\n"
              "a = AI.running_ai[0]\n"
              "def _nop(self):\n    pass\n"
              "a.__class__.Think = _nop\n"
              "a.tactics = []\n"
              "a.Throttle(0)\n"
              "a.Turn(0)\n")
    print("neuter scout:", cd.py(neuter))
    hook = subprocess.Popen([cd.PY, os.path.join(cd.HERE, "ra2_hook.py"),
                             os.path.join(cd.HERE, "frida", "ra2_damage.js"),
                             "--seconds", str(seconds)], stdout=subprocess.PIPE, text=True)
    time.sleep(1.0)
    sampler = subprocess.Popen([cd.PY, os.path.join(cd.HERE, "ra2_python.py"), "--sample", EXPR,
                                "--every", "3", "--seconds", str(seconds - 1)],
                               stdout=subprocess.PIPE, text=True)
    rows = sampler.communicate()[0]
    dmg = hook.communicate()[0]
    n = 0
    base = os.path.join(cd.ORIG, "weapon-alarm-vs-inert-scout__live__%03d")
    while os.path.exists(base % n + ".tsv"):
        n += 1
    open(base % n + ".tsv", "w").write(
        "time_elapsed\t[(player, location, heading, speed, hitpoints(p,0), health(p,0), [health per component])]\n" + rows)
    open(base % n + ".jsonl", "w").write(dmg)
    print("rows", len(rows.splitlines()), "damage events", len(dmg.splitlines()), "->", base % n)
    cd.py("import plus; plus.eliminatePlayer(1)")
    time.sleep(6)
    cd.drive(["click", "717", "575", "hide", "wait", "3"])


if __name__ == "__main__":
    main()
