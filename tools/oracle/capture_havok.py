#!/usr/bin/env python3
"""Havok step/backstep/hinge-motor measurement on the ORIGINAL during the drive test.

Needs `tools/capture/original_session.sh start` and the exhibition set up as for
capture_drive.py.  Loads frida/ra2_havok_step.js, runs the drive test (turn,
coast, straight, stop) with the AI neutered, and writes
  local/oracle/original/havok-step__live__<seq>.jsonl   raw batched event rows
  local/oracle/original/havok-step__live__<seq>.world.json  world object / sampler info
  local/oracle/original/havok-step__live__<seq>.speed.tsv   plus.* trace (speed, location)
Use MODE=idle to skip the drive plan (just record N seconds of whatever runs).
"""
import json
import os
import subprocess
import sys
import time

import frida

import capture_drive as cd

HERE = cd.HERE
AGENT = os.path.join(HERE, "frida", "ra2_havok_step.js")
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


def main():
    mode = os.environ.get("MODE", "drive")
    if os.environ.get("SKIP_START") != "1":
        cd.start_match(os.environ.get("ARENA_ROW_Y", "188"))
        print("neuter:", cd.py(cd.NEUTER))
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    rows = []
    worlds = set()

    def on_msg(m, d):
        if m.get("type") == "send":
            p = m["payload"]
            if p.get("ev") == "batch":
                rows.extend(p["rows"])
        else:
            print("agent error", m, file=sys.stderr)

    sc = sess.create_script('var HOOKS = %s;\n' % json.dumps(os.environ.get('HOOKS', 'FSIAVHTLMC')) + open(AGENT).read())
    sc.on("message", on_msg)
    sc.load()
    n = 0
    while os.path.exists(os.path.join(cd.ORIG, f"havok-step__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(cd.ORIG, f"havok-step__live__{n:03d}")
    secs = float(os.environ.get("SECONDS", "20"))
    sampler = subprocess.Popen(
        [cd.PY, os.path.join(HERE, "ra2_python.py"), "--sample", cd.EXPR, "--every", "3",
         "--seconds", str(secs)], stdout=subprocess.PIPE, text=True)
    time.sleep(3.5)
    t_start = time.time()
    if mode == "straight":
        plan = [("straight", "import AI\nAI.running_ai[0].Throttle(100)\nAI.running_ai[0].Turn(0)", 2.0),
                ("stop", "import AI\nAI.running_ai[0].Throttle(0)\nAI.running_ai[0].Turn(0)", 3.0)]
        marks = []
        for name, code, hold in plan:
            t = time.time()
            cd.py(code)
            marks.append((name, t - t_start))
            time.sleep(max(0, hold - (time.time() - t)))
    elif mode == "drive":
        plan = [("turn", "import AI\nAI.running_ai[0].Throttle(0)\nAI.running_ai[0].Turn(100)", 3.0),
                ("coast", "import AI\nAI.running_ai[0].Throttle(0)\nAI.running_ai[0].Turn(0)", 1.5),
                ("straight", "import AI\nAI.running_ai[0].Throttle(100)\nAI.running_ai[0].Turn(0)", 3.0),
                ("stop", "import AI\nAI.running_ai[0].Throttle(0)\nAI.running_ai[0].Turn(0)", 3.0)]
        marks = []
        for name, code, hold in plan:
            t = time.time()
            r = cd.py(code)
            marks.append((name, t - t_start))
            time.sleep(max(0, hold - (time.time() - t)))
        print("marks", marks)
    else:
        time.sleep(secs - 3.5)
    text = sampler.communicate()[0]
    time.sleep(1)
    sc.exports_sync.flush()
    time.sleep(0.5)
    info = {}
    for r in rows:
        if r[0] == "S":
            worlds.add(r[3])
    for w in worlds:
        info[w] = sc.exports_sync.world(w)
    sc.unload()
    sess.detach()
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    json.dump({"worlds": info, "mode": mode, "rows": len(rows),
               "marks": locals().get("marks")}, open(base + ".world.json", "w"), indent=1)
    with open(base + ".speed.tsv", "w") as f:
        f.write("time_elapsed\t[(player, location, heading, speed, hitpoints, health, cmd_throttle, cmd_turn)]\n")
        f.write(text)
    print("wrote", base, len(rows), "rows", info)


if __name__ == "__main__":
    main()
