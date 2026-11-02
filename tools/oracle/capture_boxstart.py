#!/usr/bin/env python3
"""The first seconds of an AI-v-AI fight in the ORIGINAL with the agent loaded
BEFORE the match starts, so every GetPath call (0x405990) from the first frame
is recorded (the older boxfight captures attached late).  Needs
`tools/capture/original_session.sh start` and the exhibition set up
(routes/exhibition_box_scout_alarm.steps).  Writes into OUT (absolute path):
  boxstart-<NAME>__live__<seq>.jsonl/.json   rows of frida/ra2_boxfight.js
  boxstart-<NAME>__live__<seq>.tsv           pose samples (ra2_python.py --sample)
"""
import json
import os
import subprocess
import sys
import time

import frida

import capture_drive as cd

HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
AGENT = os.environ.get("AGENT", os.path.join(cd.HERE, "frida", "ra2_boxfight.js"))
EXPR = ("repr([(p, plus.getLocation(p), plus.isUpsideDown(p), "
        "getattr(__import__('AI').running_ai[p].lastTactic, 'name', '-'), "
        "__import__('AI').running_ai[p].set_throttle, "
        "__import__('AI').running_ai[p].set_turn_throttle) for p in plus.getPlayers()])")


def main():
    name = os.environ.get("NAME", "scout-alarm-box")
    out_dir = os.environ["OUT"]
    assert os.path.isabs(out_dir)
    secs = float(os.environ.get("SIM_SECONDS", "25"))
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    rows, sim = [], [0.0]
    last_a = [time.time()]

    def on_msg(m, d):
        if m.get("type") == "send" and m["payload"].get("ev") == "batch":
            for r in m["payload"]["rows"]:
                if r[0] == "A":
                    sim[0] += r[2]
                    last_a[0] = time.time()
            rows.extend(m["payload"]["rows"])
        elif m.get("type") != "send":
            print("agent error", m, file=sys.stderr)

    sc = sess.create_script(open(AGENT).read())
    sc.on("message", on_msg)
    sc.load()
    if os.environ.get("AITRACE") == "1":
        print("aitrace install:", cd.run([cd.PY, os.path.join(cd.HERE, "ra2_python.py"),
                                          os.path.join(cd.HERE, "ai_trace_install.py")]).stdout)
    cd.start_match(os.environ.get("ARENA_ROW_Y", "188"))
    # PRIME=1 (frida/ra2_parts.js): ask each player's AI for GetComponentHealth(i) for
    # every id, so the agent's 0x4dd760 hook records the id -> component pointer map.
    if os.environ.get("PRIME") == "1":
        for p in (0, 1):
            sc.exports_sync.mark(p)
            cd.py("import AI\nr = AI.running_ai[%d]\nfor i in range(40):\n"
                  "    try:\n        r.GetComponentHealth(i)\n    except Exception:\n        pass\n" % p)
    # WEIGHTS=1: what the scripts see as each robot's weight (docs/29 12aj).
    weights = None
    if os.environ.get("WEIGHTS") == "1":
        weights = cd.py("import plus; out.write(repr([(p, plus.getWeight(p)) for p in plus.getPlayers()]))")
        print("getWeight:", weights)
    sampler = subprocess.Popen(
        [cd.PY, os.path.join(cd.HERE, "ra2_python.py"), "--sample", EXPR, "--every", "3",
         "--seconds", str(secs * 1.2 + 3)], stdout=subprocess.PIPE, text=True)
    t0 = time.time()
    base = sim[0]
    while sim[0] < base + secs and time.time() - t0 < secs * 6:
        # The fight is over (the physics stopped stepping 20 s ago): the results screen.
        if sim[0] > base + 5 and time.time() - last_a[0] > 20:
            break
        time.sleep(0.5)
    sc.exports_sync.flush()
    time.sleep(0.5)
    sc.unload()
    sess.detach()
    text = sampler.communicate()[0]
    n = 0
    while os.path.exists(os.path.join(out_dir, f"boxstart-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"boxstart-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    json.dump({"name": name, "sim_seconds": sim[0], "rows": len(rows), "getWeight": weights},
              open(base + ".json", "w"), indent=1)
    with open(base + ".tsv", "w") as f:
        f.write("time_elapsed\t[(player, location, isUpsideDown, lastTactic, cmd_throttle, cmd_turn)]\n")
        f.write(text)
    tr = os.path.join(cd.DRIVE_C if hasattr(cd, "DRIVE_C") else os.path.expanduser(
        "~/.var/app/org.winehq.Wine/data/wine/drive_c"), "ra2oracle", "aitrace.txt")
    if os.environ.get("AITRACE") == "1" and os.path.exists(tr):
        import shutil
        shutil.copy(tr, base + ".aitrace.txt")
    print("wrote", base, len(rows), "rows, sim", round(sim[0], 1), "s")


if __name__ == "__main__":
    main()
