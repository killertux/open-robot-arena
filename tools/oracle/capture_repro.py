#!/usr/bin/env python3
"""Is a match reproducible on the ORIGINAL?  Runs the same exhibition match
(Combat Arena, AI Scout vs AI ALARM, no intervention) N times, with or without
the seed pins, samples both robots' locations for ~25 s of fight time and
prints how far the runs drift apart.

    capture_repro.py --pin none|seed --runs 2 [--out DIR]

pin=seed: before Start Game it runs `random.seed(1234)` in the game's Python
(announcer picks, hazard timing in Arenas/*.py use the `random` module) and
loads frida/ra2_seed.js (replaces the C rand()).  Needs the exhibition set up
(routes/exhibition_box_scout_alarm.steps).  Writes
local/oracle/original/repro-<pin>__live__<n>.tsv per run.
"""
import argparse
import ast
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import capture_drive as cd  # noqa: E402

EXPR = ("repr([(p, plus.getLocation(p), plus.getHeading(p), plus.getHitpoints(p, 0)) "
        "for p in plus.getPlayers()])")


def one(pin, n):
    hook = None
    if pin == "seed":
        hook = subprocess.Popen([cd.PY, os.path.join(cd.HERE, "ra2_hook.py"),
                                 os.path.join(cd.HERE, "frida", "ra2_seed.js"), "--seconds", "120"],
                                stdout=subprocess.PIPE, text=True)
        time.sleep(3)
        cd.py("import random\nrandom.seed(1234)\nout.write('seeded')")
    # start_match waits blind for load + intro; python is only used after it
    cd.start_match()
    sampler = subprocess.Popen([cd.PY, os.path.join(cd.HERE, "ra2_python.py"), "--sample", EXPR,
                                "--every", "3", "--seconds", "25"], stdout=subprocess.PIPE, text=True)
    rows = sampler.communicate()[0]
    path = os.path.join(cd.ORIG, "repro-%s__live__%03d.tsv" % (pin, n))
    open(path, "w").write("time_elapsed\t[(player, location, heading, hitpoints)]\n" + rows)
    cd.py("import plus; plus.eliminatePlayer(1)")
    time.sleep(6)
    cd.drive(["click", "717", "575", "hide", "wait", "3"])
    if hook:
        hook.terminate()
    return path


def series(path):
    out = []
    for line in open(path).read().splitlines()[1:]:
        t, d = line.split("\t", 1)
        out.append((float(t), ast.literal_eval(d)))
    return out


def at(rows, t, p):
    """Location of robot p at game time t, linearly interpolated."""
    for (t0, d0), (t1, d1) in zip(rows, rows[1:]):
        if t0 <= t <= t1 and t1 > t0:
            f = (t - t0) / (t1 - t0)
            a, b = d0[p][1], d1[p][1]
            return tuple(x + (y - x) * f for x, y in zip(a, b))
    return rows[-1][1][p][1]


def t_abs(r, t):
    # compare at the same *game time since the fight clock reached 5 s*
    return 5.0 + t


def report(paths):
    runs = [series(p) for p in paths]
    t0 = [r[0][0] for r in runs]
    print("first sample game-time per run:", [round(x, 2) for x in t0])
    for t in (1, 2, 5, 10, 15, 19):
        for p in (0, 1):
            pts = [at(r, 5.0 + t, p) for r in runs]
            d = max(((a[0] - b[0]) ** 2 + (a[2] - b[2]) ** 2) ** .5 for a in pts for b in pts)
            print("t+%2ds robot %d: max pairwise distance %.2f m" % (t, p, d))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pin", choices=("none", "seed"), default="none")
    ap.add_argument("--runs", type=int, default=2)
    a = ap.parse_args()
    paths = [one(a.pin, i) for i in range(a.runs)]
    report(paths)


if __name__ == "__main__":
    main()
