#!/usr/bin/env python3
"""Photograph two robots of the ORIGINAL from fixed cameras (docs/29 section 12p, docs/37 section 6).

Needs `tools/capture/original_session.sh start` and the game at the main menu.  Copies
SHAPE_BOT0 / SHAPE_BOT1 (paths under local/game, default FlapJack and EMERGENCY) over the
*Wine prefix's* AI/Team0/Bot0.bot and Bot1.bot (backed up, restored, sha256 checked), sets up
the Combat Arena exhibition (hazards off), stops both AIs and takes, for each robot, the
screenshots of SHOTS = [(player, fwd, side, up, look_fwd), ...] with docs/37's camera
(`fwd` metres ahead of the robot, `side` to its right, `up` above its location, looking at the
point `look_fwd` ahead and 0.15 up, FOV 0.8).  Writes OUT/<NAME>__live__<seq>.jsonl and -<k>.png.

  NAME=shape OUT=<dir> tools/oracle/capture_shape.py
"""
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import time

import capture_drive as cd
import ra2_drive
import ra2_python

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
GAME = os.path.expanduser("~/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2")
DISPLAY = os.environ.get("RA2_CAPTURE_DISPLAY", ":5")
SRC = os.environ.get("RA2_GAME_SRC", os.path.join(ROOT, "local", "game"))

CAM = """
import plus, math
a = plus.getLocation(%(p)d)
h = plus.getHeading(%(p)d)
fx, fz = math.sin(h), math.cos(h)
rx, rz = fz, -fx
m = (a[0] + %(fwd)f * fx + %(side)f * rx, a[1] + %(up)f, a[2] + %(fwd)f * fz + %(side)f * rz)
t = (a[0] + %(look)f * fx, a[1] + 0.15, a[2] + %(look)f * fz)
d = [t[0] - m[0], t[1] - m[1], t[2] - m[2]]
L = math.sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2])
d = [d[0] / L, d[1] / L, d[2] / L]
plus.setCameraPosition(m[0], m[1], m[2])
plus.setCameraRotation(-math.asin(d[1]), math.atan2(d[0], d[2]))
plus.setCameraFOV(0.8)
out.write(repr((a, h, m, plus.getTimeElapsed())))
"""

# FlapJack: its wedge from the front corner, from the side, from the front; EMERGENCY from the
# four corners
SHOTS = [
    (0, 3.2, 1.8, 1.2, 1.2), (0, 0.0, 3.2, 0.9, 0.5), (0, 3.6, 0.0, 1.0, 1.0),
    (1, 3.2, 1.8, 1.2, 0.5), (1, 3.2, -1.8, 1.2, 0.5), (1, -3.2, 1.8, 1.2, -0.5),
    (1, -3.2, -1.8, 1.2, -0.5), (1, 0.0, 3.4, 1.0, 0.0),
]
# SHAPE_SET=audit (docs/34 section 11): any two stock robots, four cameras each (front-right corner, left
# side, back-left corner, front), the same list on both sides (examples/shapeshot.rs, RA2_SHAPE_SET=audit)
AUDIT_VIEWS = [(3.4, 2.0, 1.3, 0.5), (0.0, -3.4, 1.0, 0.0), (-3.4, -2.0, 1.4, -0.5), (3.6, 0.0, 1.0, 1.0)]
AUDIT = [(p,) + v for p in (0, 1) for v in AUDIT_VIEWS]
if os.environ.get("SHAPE_SET") == "audit":
    SHOTS = AUDIT


def start_match_early(arena_y="188", tries=3):
    """capture_drive.start_match with the AIs neutered while the intro is still up (SHAPE_EARLY=1), so the
    robots stand at their spawn points when the fight starts (docs/34 section 11): the late neuter of
    docs/34 section 10 lets them drive for the first seconds, a different place every run."""
    for _ in range(tries):
        cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", arena_y, "wait", "1.5", "hide",
                  "wait", "1", "click", "936", "647", "hide", "wait", "14"])
        print("early neuter:", cd.py(cd.NEUTER), flush=True)
        cd.drive(["click", "512", "735", "hide", "wait", "2"])
        # the fight clock starts after the countdown; the shots must finish inside the immobility
        # warning (about 12 s into the fight for two stopped robots)
        for _ in range(40):
            r = cd.py("import plus; out.write(repr(plus.getTimeElapsed()))").strip()
            try:
                if float(r) > 0.2:
                    return
            except ValueError:
                pass
        time.sleep(10)
    sys.exit("start_match_early: the match did not start")


def sha(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()


def main():
    # a hung game must not leave the prefix's robots swapped: SIGTERM and a watchdog end the run through
    # the `finally` below, which puts the bots back (and checks their sha256)
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(2))
    signal.signal(signal.SIGALRM, lambda *_: sys.exit("capture_shape: watchdog (SHAPE_WATCHDOG s) expired"))
    signal.alarm(int(os.environ.get("SHAPE_WATCHDOG", "420")))
    name = os.environ.get("NAME", "shape")
    out_dir = os.environ.get("OUT", cd.ORIG)
    bots = [os.environ.get("SHAPE_BOT0", "AI/Team8/Bot0.bot"), os.environ.get("SHAPE_BOT1", "AI/Team5/Bot2.bot")]
    t0 = os.path.join(GAME, "AI", "Team0")
    paths = [os.path.join(t0, f"Bot{i}.bot") for i in (0, 1)]
    sums = [sha(p) for p in paths]
    for p in paths:
        shutil.copy2(p, p + ".shape-backup")
    py = None
    rows = []
    try:
        for p, b in zip(paths, bots):
            if b:  # empty SHAPE_BOTn keeps the prefix's own robot (Scout / ALARM)
                shutil.copy2(os.path.join(SRC, b), p)
        cd.drive(["-f", os.path.join(HERE, "routes", "exhibition_box_scout_alarm.steps")])
        if os.environ.get("SHAPE_EARLY"):
            start_match_early("188")
        else:
            cd.start_match("188")
            print("neuter:", cd.py(cd.NEUTER), flush=True)
        hook = None
        if os.environ.get("SHAPE_HOOK"):  # docs/34 section 10: dump the D3D8 state while the shots are taken
            hook = subprocess.Popen([cd.PY, os.path.join(HERE, "ra2_hook.py"), os.path.join(HERE, "frida", os.environ.get("SHAPE_HOOK_JS", "ra2_d3dstate.js")),
                                     "--seconds", os.environ.get("SHAPE_HOOK_SECONDS", "14")],
                                    stdout=open(os.environ["SHAPE_HOOK"], "w"))
            time.sleep(3.0)
        n = 0
        while os.path.exists(os.path.join(out_dir, f"{name}__live__{n:03d}.jsonl")):
            n += 1
        base = os.path.join(out_dir, f"{name}__live__{n:03d}")
        os.makedirs(out_dir, exist_ok=True)
        time.sleep(0.3 if os.environ.get("SHAPE_EARLY") else 1.0)
        for k, (p, fwd, side, up, look) in enumerate(SHOTS):
            r = cd.py(CAM % dict(p=p, fwd=fwd, side=side, up=up, look=look))
            time.sleep(float(os.environ.get("SHAPE_SETTLE", "0.5")))
            path = f"{base}-{k:02d}.png"
            ra2_drive.shot(DISPLAY, path)
            rows.append({"ev": "shot", "file": os.path.basename(path), "player": p, "fwd": fwd, "side": side,
                         "up": up, "look": look, "pose": r.strip(), "bot": bots[p]})
        with open(base + ".jsonl", "w") as fh:
            for r in rows:
                fh.write(json.dumps(r, separators=(",", ":")) + "\n")
        print("wrote", base, len(rows))
        if hook:
            hook.wait(timeout=60)
        cd.py("import plus; plus.eliminatePlayer(1)")
        time.sleep(6)
        cd.drive(["click", "717", "575", "hide", "wait", "3", "click", "527", "745", "hide", "wait", "3"])
    finally:
        for p, s in zip(paths, sums):
            shutil.move(p + ".shape-backup", p)
            if sha(p) != s:
                print("WARNING: not restored", p, file=sys.stderr)


if __name__ == "__main__":
    main()
