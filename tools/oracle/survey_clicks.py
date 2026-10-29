#!/usr/bin/env python3
"""Which clicks play Sounds\\button_click.wav in the ORIGINAL?

    survey_clicks.py NAME STEPS.tsv

STEPS.tsv: one click per line, `label<TAB>x<TAB>y<TAB>wait_seconds` (game pixels).
Runs capture_audio.py for the duration, clicks each point through ra2_drive.py,
and writes <OUT>/audio-NAME__live__NNN.clicks.tsv:
`label  x  y  t_before_ms  t_after_ms  plays(0/1)  ui_calls`, where plays is
whether the sound log has a `U` (UI load) row between the two stamps.
"""
import json
import os
import subprocess
import sys
import time

import ra2_drive

HERE = os.path.dirname(os.path.abspath(__file__))
PY = os.environ.get("RA2_FRIDA_PYTHON", os.path.expanduser("~/tools/frida-venv/bin/python3"))
ORIG = os.path.join(os.path.dirname(os.path.dirname(HERE)), "local", "oracle", "original")


def main():
    name, steps_file = sys.argv[1], sys.argv[2]
    out_dir = os.environ.get("OUT", ORIG)
    steps = [l.rstrip("\n").split("\t") for l in open(steps_file) if l.strip() and not l.startswith("#")]
    total = sum(float(s[3]) + 3 for s in steps) + 10
    n = 0
    while os.path.exists(os.path.join(out_dir, f"audio-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"audio-{name}__live__{n:03d}")
    cap = subprocess.Popen([PY, os.path.join(HERE, "capture_audio.py"), name, str(total)], env=dict(os.environ, OUT=out_dir))
    time.sleep(5)
    marks = []
    # one attach for the whole survey: repeated attaches wedge frida-server
    session, api = ra2_drive.connect(os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142"), "Robot Arena 2.exe")
    display = os.environ.get("RA2_CAPTURE_DISPLAY", ":5")
    for label, x, y, wait in steps:
        t0 = int(time.time() * 1000)
        ra2_drive.run(api, display, ["click", x, y, "wait", wait, "hide"])
        t1 = int(time.time() * 1000)
        marks.append((label, x, y, t0, t1))
        print(label, flush=True)
    session.detach()
    open(base + ".stop", "w").close()
    cap.wait()
    os.remove(base + ".stop")
    rows = [json.loads(l) for l in open(base + ".jsonl")][1:]
    ui = [r for r in rows if r[0] == "U"]
    with open(base + ".clicks.tsv", "w") as f:
        f.write("label\tx\ty\tt_before_ms\tt_after_ms\tplays\tui_calls\n")
        for label, x, y, t0, t1 in marks:
            calls = [r[3] for r in ui if t0 <= r[2] <= t1 + 300]
            f.write(f"{label}\t{x}\t{y}\t{t0}\t{t1}\t{1 if calls else 0}\t{','.join(calls)}\n")
    print("wrote", base + ".clicks.tsv")


if __name__ == "__main__":
    main()
