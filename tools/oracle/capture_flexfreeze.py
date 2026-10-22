#!/usr/bin/env python3
"""FlexTop's table after both robots fell off (docs/29 section 12j): what stops it.

Needs `tools/capture/original_session.sh start`, the exhibition set to Tabletop / FlexTop with
two robots (the caller clicks them in) and the game on the setup screen.  Starts the match, stops
both AIs, drives both robots off the edge by hand, and records `frida/ra2_flexend.js` for
SIM_SECONDS of Havok time:   OUT=<dir> NAME=<n> SIM_SECONDS=45 tools/oracle/capture_flexfreeze.py
Writes OUT/flexend-<NAME>__live__<seq>.jsonl (the agent's rows, `Y` rows = host-side samples).
"""
import json
import math
import os
import sys
import threading
import time

import frida

import capture_drive as cd
import ra2_python

HERE = os.path.dirname(os.path.abspath(__file__))
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


def main():
    name = os.environ.get("NAME", "scout-alarm-flextop-off")
    out_dir = os.environ.get("OUT", cd.ORIG)
    secs = float(os.environ.get("SIM_SECONDS", "45"))
    cd.start_match(os.environ.get("ARENA_ROW_Y", "188"))
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    rows, lock, sim = [], threading.Lock(), [0.0]

    def on_msg(m, d):
        if m.get("type") != "send":
            print("agent error", m, file=sys.stderr)
            return
        with lock:
            for r in m["payload"]["rows"]:
                if r[0] == "A":
                    sim[0] += r[2]
                rows.append(r)

    src = open(os.path.join(HERE, "frida", "ra2_flexend.js")).read()
    if os.environ.get("FLEXEND_CALLS"):
        src = "const FLEXEND_CALLS = true;\n" + src
    sc = sess.create_script(src)
    sc.on("message", on_msg)
    sc.load()
    print("neuter:", cd.py(cd.NEUTER), flush=True)
    psess, api = ra2_python.connect(HOST, "Robot Arena 2.exe")
    os.makedirs(ra2_python.OUT_DIR, exist_ok=True)
    path = os.path.join(ra2_python.OUT_DIR, "ff_out.txt")

    def run(src):
        body = "\n".join("        " + line for line in src.splitlines())
        if os.path.exists(path):
            os.remove(path)
        t = api.exec(ra2_python.WRAP % {"path": ra2_python.WIN_OUT + "ff_out.txt", "body": body})
        end = time.time() + 15
        while api.result(t) is None and time.time() < end:
            time.sleep(0.02)
        return open(path).read() if os.path.exists(path) else ""

    t0 = time.time()
    # which way does Turn(+) turn each robot (heading is atan2(x, z) of the forward vector)?
    h0 = eval(run("import plus\nout.write(repr([plus.getHeading(p) for p in plus.getPlayers()]))"))
    run("import AI\nAI.running_ai[0].Turn(100)\nAI.running_ai[1].Turn(100)")
    time.sleep(0.6)
    h1 = eval(run("import plus\nout.write(repr([plus.getHeading(p) for p in plus.getPlayers()]))"))
    run("import AI\nAI.running_ai[0].Turn(0)\nAI.running_ai[1].Turn(0)")
    sign = [1.0 if (b - a + math.pi) % (2 * math.pi) - math.pi > 0 else -1.0 for a, b in zip(h0, h1)]
    print("turn signs", sign, h0, h1, flush=True)
    last = [None, None]
    while sim[0] < secs and time.time() - t0 < secs * 6:
        s = run("import plus\nout.write(repr([(plus.getLocation(p), plus.getHeading(p)) for p in plus.getPlayers()]))")
        try:
            st = eval(s)
        except Exception:
            continue
        with lock:
            rows.append(["Y", 0, round(sim[0], 3), st])
        for p, (loc, h) in enumerate(st):
            if loc[1] < -3:
                run("import AI\nAI.running_ai[%d].Throttle(0)\nAI.running_ai[%d].Turn(0)" % (p, p))
                continue
            # away from the table's centre along the nearest diagonal (the rim boxes sit on the axes)
            want = (round((math.atan2(loc[0], loc[2]) - math.pi / 4) / (math.pi / 2)) * (math.pi / 2)) + math.pi / 4
            err = (want - h + math.pi) % (2 * math.pi) - math.pi
            turn = max(-100, min(100, err * 120)) * sign[p]
            thr = 0 if abs(err) > 0.6 else 100
            run("import AI\nAI.running_ai[%d].Turn(%d)\nAI.running_ai[%d].Throttle(%d)" % (p, turn, p, thr))
    sc.exports_sync.flush()
    time.sleep(0.5)
    sc.unload()
    sess.detach()
    psess.detach()
    n = 0
    while os.path.exists(os.path.join(out_dir, f"flexend-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"flexend-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    print("wrote", base, len(rows), "rows, sim", round(sim[0], 1))


if __name__ == "__main__":
    main()
