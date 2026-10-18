#!/usr/bin/env python3
"""Capture stills of the stock arenas from the ORIGINAL game (needs a live
`tools/capture/original_session.sh start`, frida venv python).

For each arena row of the Exhibition > Arena list (DEATHMATCH, two Red Zone AI
robots, Scout vs ALARM -- set up first with routes/exhibition_box_scout_alarm.steps
or any state with both Robots slots filled) it captures, at 1024x768:

    arena-<id>-select     the Arena tab with that row selected (preview + text)
    arena-<id>-intro      the arena intro fly-by (~2.5 s after Start Game)
    arena-<id>-countdown  after Skip Intro
    arena-<id>-f2,f3,f3b,f4  cameras after F2, F3, F3 again, F4, ~12 s into the fight
                          (F1 is only the controls reminder overlay and swallows later keys)

then ends the match with plus.eliminatePlayer(1), clicks Continue on the
results screen and goes on with the next row.  Files and manifest rows are
added with add_fixture.py.  Usage:

    capture_arenas.py [--rows 0-7] [--names "bridge,box,..."] [--keep-tmp]
"""
import argparse
import os
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
PY = os.environ.get("RA2_FRIDA_PYTHON", os.path.expanduser("~/tools/frida-venv/bin/python3"))
DEFAULT_NAMES = ["bridge", "box", "compressor", "electric", "halfcircle",
                 "metalskull", "parkinglot", "octagon"]  # list order, 1024x768 UI


def drive(steps, check=True):
    p = subprocess.run([PY, os.path.join(HERE, "ra2_drive.py")] + steps,
                       capture_output=True, text=True, timeout=240)
    if check and p.returncode:
        sys.exit(f"ra2_drive failed: {p.stderr[-500:]}")
    return p


def python(code):
    return subprocess.run([PY, os.path.join(HERE, "ra2_python.py"), "-c", code],
                          capture_output=True, text=True, timeout=120)


def add(fid, path, state):
    subprocess.run([sys.executable, os.path.join(HERE, "add_fixture.py"), fid, path, state],
                   check=True, capture_output=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rows", default="0-7")
    ap.add_argument("--names", default=",".join(DEFAULT_NAMES))
    ap.add_argument("--keep-tmp", action="store_true")
    a = ap.parse_args()
    lo, hi = (int(x) for x in a.rows.split("-"))
    names = a.names.split(",")
    tmp = tempfile.mkdtemp(prefix="ra2-arenas-")
    for row in range(lo, hi + 1):
        name = names[row] if row < len(names) else f"row{row}"
        y = 170 + 18 * row
        base = f"EXHIBITION > Arena row {row} ({name}), DEATHMATCH, Red Zone Scout vs ALARM AI, hazards on"
        shots = {k: os.path.join(tmp, f"{name}-{k}.png")
                 for k in ("select", "intro", "countdown", "f2", "f3", "f3b", "f4")}
        steps = [
            "click", "176", "177", "wait", "1.5",
            "click", "322", str(y), "wait", "1.5", "hide", "wait", "1.5",
            "shot", shots["select"],
            "click", "936", "647", "hide", "wait", "2.5", "shot", shots["intro"],
            "click", "512", "735", "hide", "wait", "1.5", "shot", shots["countdown"],
            "wait", "10",
        ]
        for k in ("f2", "f3", "f3b", "f4"):
            steps += ["key", k[:2].upper(), "wait", "1.5", "shot", shots[k]]
        drive(steps)
        add(f"arena-{name}-select", shots["select"], base + "; picker with the row selected")
        add(f"arena-{name}-intro", shots["intro"], base + "; intro fly-by ~2.5 s after Start Game")
        add(f"arena-{name}-countdown", shots["countdown"], base + "; after Skip Intro (countdown)")
        for k in ("f2", "f3", "f3b", "f4"):
            add(f"arena-{name}-{k}", shots[k], base + f"; match camera key {k[:2].upper()}{" (second press)" if k.endswith("b") else ""} ~12 s into the fight")
        python("import plus; plus.eliminatePlayer(1)")
        time.sleep(6)
        drive(["click", "717", "575", "hide", "wait", "3"])
        print(f"row {row} {name}: done", flush=True)
    if not a.keep_tmp:
        for f in os.listdir(tmp):
            os.unlink(os.path.join(tmp, f))
        os.rmdir(tmp)


if __name__ == "__main__":
    main()
