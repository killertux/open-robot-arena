#!/usr/bin/env python3
"""The robots' first steps in a match of the ORIGINAL: where they start and how
far they drop (docs/29 section 12r).

Needs `tools/capture/original_session.sh start` and the exhibition set up at its
setup screen (capture_tipab.py setup does it, or the route plus the arena tab).
Loads frida/ra2_tipab.js *before* the match starts, so its MagicAction hook
learns each chassis at its first step and the B rows follow it from there;
then Start Game, the intro skipped after INTRO s (default 14), SECONDS of wall
time in all (default 30). Writes OUT (default local/oracle/original)
spawn-<NAME>__live__<seq>.jsonl / .json (NAME default scout-alarm-box).
"""
import json
import os
import time

import capture_drive as cd
import capture_tipab as ct


def main():
    name = os.environ.get("NAME", "scout-alarm-box")
    out_dir = os.environ.get("OUT", cd.ORIG)
    ag = ct.Agent()
    t0 = time.time()
    cd.drive(["click", "936", "647", "hide"])
    marks = [("start", time.time() - t0)]
    time.sleep(float(os.environ.get("INTRO", "14")))
    cd.drive(["click", "512", "735", "hide"])
    marks.append(("skip intro", time.time() - t0))
    time.sleep(max(0.0, float(os.environ.get("SECONDS", "30")) - (time.time() - t0)))
    ag.close()
    n = 0
    while os.path.exists(os.path.join(out_dir, f"spawn-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"spawn-{name}__live__{n:03d}")
    with open(base + ".jsonl", "w") as f:
        for r in ag.rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    json.dump({"name": name, "marks": marks, "rows": len(ag.rows)}, open(base + ".json", "w"), indent=1)
    print("wrote", base, len(ag.rows), "rows", marks)


if __name__ == "__main__":
    main()
