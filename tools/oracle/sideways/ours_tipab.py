#!/usr/bin/env python3
"""ours_tipab.py <grid.json> <out.jsonl>: a tipab grid (capture_tipab.py) run in ours with examples/tipprobe, the B rows of each trial in
the live files' form, TRIAL rows in front (docs/29 12bd). Run from engine/ after `cargo build --release -p ra2sim --example tipprobe`.
The live substep is 1/60 s; a trial's `steps`, `drive_py` and `fire_py` are converted to seconds / ours' 1/90 s steps. An `extra` group
(the second robot placed `dist` m away at azimuth `az` degrees, its Fire pressed at `fire_py`) uses TIPPROBE_ALARM.
GAME (default ../local/game) is only read."""
import json
import os
import subprocess
import sys

grid = json.load(open(sys.argv[1]))
out = sys.argv[2]
game = os.environ.get("GAME", "../local/game")
dump = "/tmp/_ours_tipab.jsonl"
with open(out, "w") as f:
    for t in grid["trials"]:
        env = dict(os.environ, TIPPROBE_ROBOT="0", RA2_H17_DUMP=dump)
        env["TIPPROBE_COM_LIFT"] = str(t["dcentre"][1])
        env["TIPPROBE_COM_SHIFT"] = "%s,%s" % (t["dcentre"][0], t["dcentre"][2])
        a = t["axis"]
        secs = t.get("steps", 300) / 60.0
        args = ["./target/release/examples/tipprobe", game + "/Arenas/" + os.environ.get("ARENA", "box") + ".py", game + "/AI/Team0/Bot0.bot",
                game + "/AI/Team0/Bot1.bot", "self", str(a[0]), str(a[1]), str(a[2]), str(t["deg"]), str(t.get("w", 0)), str(secs)]
        if "extra" in t:
            ex = t["extra"]
            env["TIPPROBE_ALARM"] = "%s,%s,%s" % (ex["az"], ex["dist"], t["fire_py"] / 60.0)
        elif "fire_py" in t:
            args += ["0.0", str(t["fire_py"] / 60.0)]
        if "drive_py" in t:
            env["TIPPROBE_DRIVE"] = ",".join("%d:%d:%d" % (round(s * 1.5), th, tu) for s, th, tu in t["drive_py"])
        subprocess.run(args, env=env, capture_output=True)
        f.write(json.dumps(["TRIAL", 0, t]) + "\n")
        for line in open(dump):
            if line.startswith('["B"'):
                f.write(line)
