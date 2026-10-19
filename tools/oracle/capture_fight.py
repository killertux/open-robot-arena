#!/usr/bin/env python3
"""Every priced collision hit of an AI-vs-AI fight in the ORIGINAL.

Needs `tools/capture/original_session.sh start` and the exhibition set up
(routes/exhibition_box_scout_alarm.steps; other robots copied into the Red
Zone slots as capture_contacts_set.sh does).  Starts the match (unless
SKIP_START=1), loads frida/ra2_fight_hits.js and lets both shipped AIs fight
for SIM_SECONDS of Havok time (default 120, counted from the step pieces),
then writes into OUT (default local/oracle/original):
  fight-<name>__live__<seq>.jsonl   raw rows (see the agent's header)
  fight-<name>__live__<seq>.json    summary
`fight_report.py` reads it.
"""
import json
import os
import sys
import time

import frida

import capture_drive as cd

AGENT = os.path.join(cd.HERE, "frida", "ra2_fight_hits.js")
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


def main():
    name = os.environ.get("NAME", "scout-alarm-box")
    out_dir = os.environ.get("OUT", cd.ORIG)
    secs = float(os.environ.get("SIM_SECONDS", "120"))
    if os.environ.get("SKIP_START") != "1":
        cd.start_match(os.environ.get("ARENA_ROW_Y", "188"))
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    rows = []
    sim = [0.0]

    def on_msg(m, d):
        if m.get("type") == "send" and m["payload"].get("ev") == "batch":
            for r in m["payload"]["rows"]:
                if r[0] == "A":
                    sim[0] += r[2]
            rows.extend(m["payload"]["rows"])
        elif m.get("type") != "send":
            print("agent error", m, file=sys.stderr)

    sc = sess.create_script(open(AGENT).read())
    sc.on("message", on_msg)
    sc.load()
    t0 = time.time()
    while sim[0] < secs and time.time() - t0 < secs * 6:
        time.sleep(1.0)
    sc.exports_sync.flush()
    time.sleep(0.5)
    sc.unload()
    sess.detach()
    n = 0
    while os.path.exists(os.path.join(out_dir, f"fight-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"fight-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    json.dump({"name": name, "sim_seconds": sim[0], "wall_seconds": time.time() - t0, "rows": len(rows)},
              open(base + ".json", "w"), indent=1)
    print("wrote", base, len(rows), "rows, sim", round(sim[0], 1), "s")


if __name__ == "__main__":
    main()
