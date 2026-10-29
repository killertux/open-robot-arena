#!/usr/bin/env python3
"""Havok's MOPP compiler in the ORIGINAL: the trees it builds for the arenas'
fixed meshes, and (QUERIES=1) the order its queries return triangles in
(docs/29 section 12v).

Needs `tools/capture/original_session.sh start` and the exhibition set up with
two robots (routes/exhibition_box_scout_alarm.steps).  Loads frida/ra2_mopp.js
for the whole run, then for every arena row given starts a match (the arena is
loaded and its meshes compiled), lets it run FIGHT seconds (default 8), ends it
(the other robot eliminated) and dismisses the results.

    capture_mopp.py <out.jsonl> <rowY> [<rowY> ...]

Rows of the 1024x768 Arena list are at y = 170 + 18 * row (bridge, box,
compressor, electric, halfcircle, metalskull, parkinglot, octagon).  Each
match's rows are preceded by an `["ARENA", <rowY>]` row.
"""
import json
import os
import sys
import time

import frida

import capture_drive as cd

HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
AGENT = os.path.join(cd.HERE, "frida", "ra2_mopp.js")


def main():
    out, ys = sys.argv[1], sys.argv[2:]
    fight = float(os.environ.get("FIGHT", "8"))
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    f = open(out, "w")

    def on_msg(m, d):
        if m.get("type") == "send" and m["payload"].get("ev") == "batch":
            for r in m["payload"]["rows"]:
                f.write(json.dumps(r) + "\n")
        elif m.get("type") != "send":
            print("agent error", m, file=sys.stderr)

    src = ("const MOPP_QUERIES = %s;\n" % ("true" if os.environ.get("QUERIES") == "1" else "false")) + open(AGENT).read()
    sc = sess.create_script(src)
    sc.on("message", on_msg)
    sc.load()
    try:
        for y in ys:
            f.write(json.dumps(["ARENA", int(y)]) + "\n")
            cd.start_match(y)
            time.sleep(fight)
            sc.exports_sync.flush()
            cd.py("import plus; plus.eliminatePlayer(1)")
            time.sleep(6)
            cd.drive(["click", "717", "575", "hide", "wait", "3"])
            print("row", y, "done", flush=True)
    finally:
        sc.exports_sync.flush()
        time.sleep(0.5)
        sc.unload()
        sess.detach()
        f.close()


if __name__ == "__main__":
    main()
