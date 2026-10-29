#!/usr/bin/env python3
"""A Steering unit's wheel constraints in motion, and the build's detector
wakes, in the ORIGINAL (docs/29 section 12y).

As capture_rest.py (box, hazards off, the robots copied over the Wine prefix's
AI/Team0/Bot0.bot and Bot1.bot and restored after, AIs neutered during the
intro), with frida/ra2_wheel.js loaded before Start Game and
capture_deact.py's phases driving robot 0 (PHASES, `steps:th0:th1[:turn0:turn1]`,
the first one long enough to hold the countdown and the bell).

    OUT=<dir> PHASES="1500:0:0 360:60:0 ..." capture_wheel.py AI/Team0/Bot2.bot,AI/Team7/Bot0.bot

Writes OUT/wheel-<name>__live__<seq>.jsonl (the agent's rows, then a `DEACT`
row naming the deact capture) and deact-wheel-<name>__live__<seq>.jsonl.
"""
import json
import os
import shutil
import signal
import subprocess
import sys
import time

import capture_drive as cd
import capture_rest as cr


def one(pair, out_dir):
    name = "-".join(os.path.basename(os.path.dirname(b)).lower() + os.path.basename(b)[:-4].lower() for b in pair)
    cd.drive(["-f", os.path.join(cr.HERE, "routes", "exhibition_box_scout_alarm.steps")])
    cr.hazards_off()
    n = 0
    while os.path.exists(os.path.join(out_dir, f"wheel-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"wheel-{name}__live__{n:03d}")
    watch = subprocess.Popen([cd.PY, os.path.join(cr.HERE, "ra2_hook.py"),
                              os.path.join(cr.HERE, "frida", "ra2_wheel.js"),
                              "--seconds", os.environ.get("WATCH_SECONDS", "150"),
                              "--host", os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")],
                             stdout=open(base + ".jsonl", "w"))
    time.sleep(3.0)
    cd.drive(["click", "936", "647", "hide", "wait", "14"])
    print("early neuter:", cd.py(cd.NEUTER), flush=True)
    phases = os.environ.get("PHASES", "1800:0:0").split()
    deact = subprocess.Popen([cd.PY, os.path.join(cr.HERE, "capture_deact.py"), "run", f"wheel-{name}"] + phases,
                             env=dict(os.environ, OUT=out_dir))
    time.sleep(4.0)
    cd.drive(["click", "512", "735", "hide"])
    deact.wait(timeout=1200)
    watch.wait(timeout=600)
    with open(base + ".jsonl", "a") as f:
        f.write(json.dumps(["DEACT", f"deact-wheel-{name}"]) + "\n")
    cd.py("import plus; plus.eliminatePlayer(1)")
    time.sleep(6)
    cd.drive(["click", "717", "575", "hide", "wait", "3", "click", "527", "745", "hide", "wait", "3"])
    print("pair", name, "done", flush=True)


def main():
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(2))
    out_dir = os.environ.get("OUT", cd.ORIG)
    t0 = os.path.join(cr.GAME, "AI", "Team0")
    paths = [os.path.join(t0, f"Bot{i}.bot") for i in (0, 1)]
    sums = [cr.sha(p) for p in paths]
    for p in paths:
        shutil.copy2(p, p + ".wheel-backup")
    try:
        for spec in sys.argv[1:]:
            pair = spec.split(",")
            for p, b in zip(paths, pair):
                shutil.copy2(os.path.join(cr.SRC, b), p)
            one(pair, out_dir)
    finally:
        for p, s in zip(paths, sums):
            shutil.move(p + ".wheel-backup", p)
            if cr.sha(p) != s:
                print("WARNING: not restored", p, file=sys.stderr)


if __name__ == "__main__":
    main()
