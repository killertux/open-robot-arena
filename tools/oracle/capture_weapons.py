#!/usr/bin/env python3
"""Weapon spin-up / strike timing on the ORIGINAL (docs/29 section 12).

Needs `tools/capture/original_session.sh start` and a match already running
with the AIs neutered (capture_weapons_set.sh does both).  Loads
frida/ra2_weapons.js, then for each step of PLAN presses a control of a robot
through its AI (`AI.running_ai[p].Input(name, 0, value)`) and holds it.

  PLAN="0:Spin:1:6;0:Spin:0:5"   player:control:value:seconds;...
  NAME=weapons-tornado-ripblade  output id

Writes local/oracle/original/<NAME>__live__<n>.jsonl (agent rows plus `X`
rows marking each plan step at the substep it was issued) and a .json summary.
"""
import json
import os
import sys
import time

import frida

import capture_drive as cd

AGENT = os.environ.get("AGENT", os.path.join(cd.HERE, "frida", "ra2_weapons.js"))
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


def main():
    name = os.environ.get("NAME", "weapons")
    plan = [s.split(":") for s in os.environ.get("PLAN", "0:Spin:1:6;0:Spin:0:5").split(";") if s]
    out_dir = os.environ.get("OUT", cd.ORIG)
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    rows = []

    def on_msg(m, _d):
        if m.get("type") == "send" and m["payload"].get("ev") == "batch":
            rows.extend(m["payload"]["rows"])
        elif m.get("type") != "send":
            print("agent error", m, file=sys.stderr)

    sc = sess.create_script(open(AGENT).read())
    sc.on("message", on_msg)
    sc.load()
    time.sleep(1.0)
    for player, control, value, secs in plan:
        step = sc.exports_sync.flush()
        rows.append(["X", step, int(player), control, float(value)])
        cd.py("import AI\nAI.running_ai[%d].Input(%r, 0, %s)" % (int(player), control, value))
        # Hold for the plan's seconds of *sim* time (Frida slows the game).
        while (sc.exports_sync.flush() - step) < float(secs) * 60:
            time.sleep(0.2)
    sc.exports_sync.flush()
    time.sleep(0.5)
    sc.unload()
    sess.detach()
    n = 0
    while os.path.exists(os.path.join(out_dir, f"{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    json.dump({"name": name, "plan": plan, "rows": len(rows)}, open(base + ".json", "w"), indent=1)
    print("wrote", base, len(rows), "rows")


if __name__ == "__main__":
    main()
