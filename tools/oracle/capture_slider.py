#!/usr/bin/env python3
"""The arenas' prismatic controllers in the ORIGINAL (docs/29 section 12y):
the Octagon's pit floors, the Metal Skull's grate, the Combat Arena's floor
saw and spikes.

Needs `tools/capture/original_session.sh start` and the game at the main menu.
Sets up the exhibition (routes/exhibition_box_scout_alarm.steps: the prefix's
own Scout v ALARM, hazards as the setup screen has them), and for every
`name:row_y:seconds[:t=python;t=python...]` starts a match on that arena row
(row y = 170 + 18 · row), loads frida/ra2_slider.js, starts the match, neuters both AIs, waits
`seconds` of wall time, running each python line (through ra2_python.py) at
its wall time `t` after the agent is loaded, then ends the match.

    OUT=<dir> capture_slider.py "octagon:296:60" \
        "box:188:20:3=import Arenas;Arenas.currentArena.saw.MoveSaw(1);;9=import Arenas;Arenas.currentArena.saw.MoveSaw(-1)"

Writes OUT/slider-<name>__live__<seq>.jsonl (the agent's rows).
"""
import json
import os
import sys
import time

import frida

import capture_drive as cd

HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
AGENT = os.path.join(cd.HERE, "frida", "ra2_slider.js")


def one(spec, out_dir):
    parts = spec.split(":", 3)
    name, row_y, secs = parts[0], parts[1], float(parts[2])
    actions = []
    if len(parts) > 3:
        for a in parts[3].split(";;"):
            t, code = a.split("=", 1)
            actions.append((float(t), code.replace(";", "\n")))
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
    # The agent is in before the match loads, so its substep count runs from
    # the load; the match is started and both AIs neutered after.
    cd.start_match(row_y)
    print("neuter:", cd.py(cd.NEUTER), flush=True)
    rows.append(["NEUTER", sc.exports_sync.flush()])
    t0 = time.time()
    for t, code in sorted(actions):
        while time.time() - t0 < t:
            time.sleep(0.1)
        n = sc.exports_sync.flush()
        rows.append(["PY", n, code])
        print("py", t, cd.py(code + "\nout.write('ok')"), flush=True)
    while time.time() - t0 < secs:
        time.sleep(0.5)
    sc.exports_sync.flush()
    time.sleep(0.5)
    sc.unload()
    sess.detach()
    n = 0
    while os.path.exists(os.path.join(out_dir, f"slider-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"slider-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    print("wrote", base, len(rows), "rows", flush=True)
    cd.py("import plus; plus.eliminatePlayer(1)")
    time.sleep(6)
    cd.drive(["click", "717", "575", "hide", "wait", "3"])


def main():
    out_dir = os.environ.get("OUT", cd.ORIG)
    cd.drive(["-f", os.path.join(cd.HERE, "routes", "exhibition_box_scout_alarm.steps")])
    for spec in sys.argv[1:]:
        one(spec, out_dir)
    cd.drive(["click", "527", "745", "hide", "wait", "3"])


if __name__ == "__main__":
    main()
