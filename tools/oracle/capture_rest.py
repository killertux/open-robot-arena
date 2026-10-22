#!/usr/bin/env python3
"""Two robots left alone from the spawn in the ORIGINAL (docs/29 section 12v):
where the match puts them, how they come down at the bell and how they creep
at rest afterwards, AIs neutered before the fight, Combat Arena, hazards off.

Needs `tools/capture/original_session.sh start` and the game at the main menu.
For every pair `<bot0>,<bot1>` (paths under local/game) the robots are copied
over the *Wine prefix's* AI/Team0/Bot0.bot and Bot1.bot (backed up first,
restored and sha256-checked at the end, as capture_shape.py does), the
exhibition is set up from the main menu (routes/exhibition_box_scout_alarm.steps,
the hazards box cleared), frida/ra2_spawnwatch.js is loaded before Start Game
(every teleport of a body, with its callers), the AIs are neutered while the
intro is up, frida/ra2_deact.js is armed (capture_deact.py, NO_DRIVE: every
robot body every substep, the deactivators) for STEPS substeps (default 1800),
the intro is skipped; then the match is ended and the game taken back to the
main menu.

    OUT=<dir> capture_rest.py AI/Team5/Bot2.bot,AI/Team6/Bot0.bot [...]

Writes OUT/rest-<name>__live__<seq>.jsonl (the spawn watch rows, then a
`DEACT` row naming the deact capture) and deact-rest-<name>__live__<seq>.jsonl.
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

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
GAME = os.path.expanduser("~/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2")
SRC = os.environ.get("RA2_GAME_SRC", os.path.join(ROOT, "local", "game"))


def sha(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()


def hazards_off():
    shot = os.path.join(os.environ.get("TMPDIR", "/tmp"), "rest-setup.png")
    cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", "188", "wait", "1", "hide", "shot", shot])
    sys.path.insert(0, HERE)
    from compare import read_png
    w, h, rgb = read_png(shot)
    v = [rgb[3 * (y * w + x) + c] for y in range(484, 493) for x in range(291, 300) for c in range(3)]
    if sum(v) / len(v) > 130:
        print("hazards box checked: clearing", flush=True)
        cd.drive(["click", "295", "488", "wait", "1", "hide"])


def one(pair, out_dir):
    name = "-".join(os.path.basename(os.path.dirname(b)).lower() + os.path.basename(b)[:-4].lower() for b in pair)
    cd.drive(["-f", os.path.join(HERE, "routes", "exhibition_box_scout_alarm.steps")])
    hazards_off()
    n = 0
    while os.path.exists(os.path.join(out_dir, f"rest-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"rest-{name}__live__{n:03d}")
    watch = subprocess.Popen([cd.PY, os.path.join(HERE, "ra2_hook.py"),
                              os.path.join(HERE, "frida", os.environ.get("WATCH_AGENT", "ra2_spawnwatch.js")),
                              "--seconds", os.environ.get("WATCH_SECONDS", "75"),
                              "--host", os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")],
                             stdout=open(base + ".jsonl", "w"))
    time.sleep(3.0)
    cd.drive(["click", "936", "647", "hide", "wait", "14"])
    print("early neuter:", cd.py(cd.NEUTER), flush=True)
    deact = subprocess.Popen([cd.PY, os.path.join(HERE, "capture_deact.py"), "run", f"rest-{name}",
                              os.environ.get("STEPS", "1800") + ":0:0"],
                             env=dict(os.environ, NO_DRIVE="1", OUT=out_dir))
    time.sleep(4.0)
    cd.drive(["click", "512", "735", "hide"])
    deact.wait(timeout=600)
    watch.wait(timeout=300)
    with open(base + ".jsonl", "a") as f:
        f.write(json.dumps(["DEACT", f"deact-rest-{name}"]) + "\n")
    cd.py("import plus; plus.eliminatePlayer(1)")
    time.sleep(6)
    cd.drive(["click", "717", "575", "hide", "wait", "3", "click", "527", "745", "hide", "wait", "3"])
    print("pair", name, "done", flush=True)


def main():
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(2))
    out_dir = os.environ.get("OUT", cd.ORIG)
    t0 = os.path.join(GAME, "AI", "Team0")
    paths = [os.path.join(t0, f"Bot{i}.bot") for i in (0, 1)]
    sums = [sha(p) for p in paths]
    for p in paths:
        shutil.copy2(p, p + ".rest-backup")
    try:
        for spec in sys.argv[1:]:
            pair = spec.split(",")
            for p, b in zip(paths, pair):
                shutil.copy2(os.path.join(SRC, b), p)
            one(pair, out_dir)
    finally:
        for p, s in zip(paths, sums):
            shutil.move(p + ".rest-backup", p)
            if sha(p) != s:
                print("WARNING: not restored", p, file=sys.stderr)


if __name__ == "__main__":
    main()
