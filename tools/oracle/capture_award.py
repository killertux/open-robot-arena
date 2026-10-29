#!/usr/bin/env python3
"""Timed capture of the award scene (docs/36 section 9.23): the pad robot's pose every UI
frame plus X screenshots at wall-clock intervals, from one frida session.

    capture_award.py --out DIR [--every 1] click 717 575 shots 8 1.0 award wait 20

* ``shots N INTERVAL PREFIX`` takes N root-window screenshots INTERVAL seconds apart
  (DIR/PREFIX-<k>.png) and appends ``k  host_time`` to DIR/shots.tsv.
* every other step is a ra2_drive step.
* DIR/pose.tsv gets one row per sampled frame: ``arena_time  host_time  heading  direction
  location  speed`` of player 0 (rows before the award arena exists fail and are dropped).
"""
import argparse
import os
import sys
import time

import frida

import ra2_drive
import ra2_python

EXPR = ("repr(('%.3f' % time.time(), plus.getHeading(0), plus.getDirection(0, 0), "
        "plus.getLocation(0), plus.getSpeed(0)))")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--display", default=os.environ.get("RA2_CAPTURE_DISPLAY", ":5"))
    ap.add_argument("--host", default=os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--every", type=int, default=1)
    ap.add_argument("steps", nargs="*")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    session, api = ra2_drive.connect(a.host, "Robot Arena 2.exe")
    with open(ra2_python.AGENT) as f:
        pyscript = session.create_script(f.read())
    pyscript.load()
    pyapi = pyscript.exports_sync
    os.makedirs(ra2_python.OUT_DIR, exist_ok=True)
    host_path = os.path.join(ra2_python.OUT_DIR, "sample.txt")
    open(host_path, "w").close()
    code = ra2_python.SAMPLE % {"path": ra2_python.WIN_OUT + "sample.txt", "expr": EXPR}
    code = code.replace("import plus", "import plus, time")
    pyapi.everyframe(code, a.every)
    shots = open(os.path.join(a.out, "shots.tsv"), "w")
    try:
        steps = list(a.steps)
        chunk = []
        i = 0
        while i < len(steps):
            if steps[i] == "shots":
                ra2_drive.run(api, a.display, chunk)
                chunk = []
                n, interval, prefix = int(steps[i + 1]), float(steps[i + 2]), steps[i + 3]
                i += 4
                for k in range(n):
                    t0 = time.time()
                    ra2_drive.shot(a.display, os.path.join(a.out, f"{prefix}-{k:02d}.png"))
                    shots.write(f"{k}\t{t0:.3f}\n")
                    shots.flush()
                    time.sleep(max(0.0, interval - (time.time() - t0)))
            else:
                chunk.append(steps[i])
                i += 1
        ra2_drive.run(api, a.display, chunk)
    finally:
        pyapi.everyframe(None, 1)
        time.sleep(0.3)
        with open(host_path) as f, open(os.path.join(a.out, "pose.tsv"), "w") as out:
            for line in f:
                if "\t(" in line and "Traceback" not in line:
                    out.write(line)
        shots.close()
        session.detach()


if __name__ == "__main__":
    main()
