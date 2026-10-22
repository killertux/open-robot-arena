#!/usr/bin/env python3
"""Log every engine-LCG draw of the original while ra2_drive steps run (docs/36 section 9.23).

    capture_lcg.py --out lcg.jsonl click 514 426 mark teamhq wait 3 ...

One frida session carries both the input agent (ra2_drive's steps) and
frida/ra2_lcg.js, so no second client is attached (two clients wedge frida-server).
`setstate N` writes the LCG state (the draws that follow are then a function of N);
`mark LABEL` records the LCG state and host time at that point of the route; every
other step is a ra2_drive step.  Output: one JSON row per draw
(`D seq site state caller ms`), seed (`S`) or mark (`M`).
"""
import argparse
import json
import os
import sys
import time

import ra2_drive

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--display", default=os.environ.get("RA2_CAPTURE_DISPLAY", ":5"))
    ap.add_argument("--host", default=os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142"))
    ap.add_argument("--out", required=True)
    ap.add_argument("steps", nargs="*")
    a = ap.parse_args()
    session, api = ra2_drive.connect(a.host, "Robot Arena 2.exe")
    out = open(a.out, "w")

    def on_message(msg, data):
        if msg.get("type") == "send":
            for row in msg["payload"].get("rows", []):
                out.write(json.dumps(row) + "\n")
            out.flush()
        else:
            print("lcg agent:", msg, file=sys.stderr)

    with open(os.path.join(HERE, "frida", "ra2_lcg.js")) as f:
        script = session.create_script(f.read())
    script.on("message", on_message)
    script.load()
    try:
        chunk = []
        for step in a.steps + ["mark", "end"]:
            if chunk and chunk[-1] in ("mark", "setstate"):
                op = chunk.pop()
                ra2_drive.run(api, a.display, chunk)
                if op == "mark":
                    script.exports_sync.mark(step)
                else:
                    print("setstate", script.exports_sync.setstate(int(step)))
                chunk = []
            else:
                chunk.append(step)
        time.sleep(0.5)
        script.exports_sync.flush()
        time.sleep(0.5)
    finally:
        script.unload()
        session.detach()
        out.close()


if __name__ == "__main__":
    main()
