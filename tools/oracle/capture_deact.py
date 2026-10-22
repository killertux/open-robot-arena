#!/usr/bin/env python3
"""Havok's deactivators on the ORIGINAL (docs/29 section 12t).

Needs `tools/capture/original_session.sh start`, then `capture_tipab.py setup`
(box Scout v ALARM, hazards off, both AIs neutered) or any running match.

  capture_deact.py run NAME [phase ...]

Each phase is `steps:throttle0:throttle1[:turn0:turn1]`: the AIs' throttles are
set, then frida/ra2_deact.js is armed for that many substeps (default one
phase of 600 substeps at rest). Writes OUT (default local/oracle/original)
deact-<NAME>__live__<seq>.jsonl (a `PHASE` row ahead of each phase's rows).
NO_DRIVE=1 leaves the AIs alone (another script drives, e.g. capture_flexfreeze.py).
"""
import json
import os
import sys
import time

import frida

import capture_drive as cd

HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
AGENT = os.path.join(cd.HERE, "frida", "ra2_deact.js")


def main():
    name = sys.argv[2]
    phases = sys.argv[3:] or ["600:0:0"]
    out_dir = os.environ.get("OUT", cd.ORIG)
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
    api = sc.exports_sync
    print("immobility limit was", api.no_immobility())
    time.sleep(1.0)
    print("systems", api.systems())
    for ph in phases:
        p = ph.split(":")
        steps, th = int(p[0]), [p[1], p[2]]
        tu = [p[3], p[4]] if len(p) > 4 else ["0", "0"]
        if not os.environ.get("NO_DRIVE"):
            cd.py("import AI\nAI.running_ai[0].Throttle(%s)\nAI.running_ai[0].Turn(%s)\n"
                  "AI.running_ai[1].Throttle(%s)\nAI.running_ai[1].Turn(%s)" % (th[0], tu[0], th[1], tu[1]))
        rows.append(["PHASE", ph])
        api.arm(steps)
        t0 = time.time()
        while api.busy() and time.time() - t0 < steps / 10 + 30:
            time.sleep(0.2)
        api.flush()
        time.sleep(0.5)
        print("phase", ph, "done", round(time.time() - t0, 1), "s", len(rows), "rows", flush=True)
    if not os.environ.get("NO_DRIVE"):
        cd.py("import AI\nfor a in AI.running_ai:\n    a.Throttle(0)\n    a.Turn(0)")
    rows.append(["BODIES", api.bodies()])
    api.flush()
    time.sleep(0.5)
    sc.unload()
    sess.detach()
    n = 0
    while os.path.exists(os.path.join(out_dir, f"deact-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"deact-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    print("wrote", base, len(rows), "rows")


if __name__ == "__main__":
    if len(sys.argv) < 3 or sys.argv[1] != "run":
        sys.exit(__doc__)
    main()
