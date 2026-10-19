#!/usr/bin/env python3
"""How the ORIGINAL assembled its robots: every scene node of both robots of a
running match, at rest (docs/29 section 8).

Needs `tools/capture/original_session.sh start` and a match running (e.g.
`capture_drive.start_match` after routes/exhibition_box_scout_alarm.steps).
The AI is neutered first so the robots sit still.  Writes
  local/oracle/original/assembly-<name>__live__<seq>.json
one file per robot: the component list, the node tree (local/world 4x4 per
node, row-major, row 3 = translation) at rest ("tree") and as built, at the
robot's first physics step ("first", only when START=1 so the agent is in
before the match loads), and the Havok bodies.

    BOTS=scout,alarm tools/oracle/capture_assembly.py     # names for players 0, 1
Set START=1 to start the match (Arena tab -> Combat Arena -> Start) first.
"""
import json
import os
import sys
import time

import frida

import capture_drive as cd

AGENT = os.path.join(cd.HERE, "frida", "ra2_assembly.js")
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


def main():
    names = os.environ.get("BOTS", "scout,alarm").split(",")
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    sc = sess.create_script(open(AGENT).read())
    sc.load()
    if os.environ.get("START") == "1":
        cd.start_match(os.environ.get("ARENA_ROW_Y", "188"))
    print("neuter:", cd.py(cd.NEUTER).strip())
    time.sleep(float(os.environ.get("SETTLE", "4")))
    players = eval(cd.py("import plus\nout.write(repr([(p, plus.getLocation(p), plus.getHeading(p)) "
                         "for p in plus.getPlayers()]))"))
    d = sc.exports_sync.dump()
    sc.unload()
    sess.detach()
    # Match robots to players by the chassis body node's position.
    for r in d["robots"]:
        w = r["tree"]["kids"][0]["world"] if r["tree"].get("kids") else None
        best = None
        for p, loc, head in players:
            if w is None:
                continue
            dist = sum((w[12 + i] - loc[i]) ** 2 for i in range(3))
            if best is None or dist < best[0]:
                best = (dist, p, loc, head)
        r["player"] = best[1] if best else None
        r["location"] = best[2] if best else None
        r["heading"] = best[3] if best else None
    for r in sorted(d["robots"], key=lambda r: r["player"] if r["player"] is not None else 9):
        name = names[r["player"]] if r["player"] is not None and r["player"] < len(names) else "robot"
        n = 0
        while os.path.exists(os.path.join(cd.ORIG, f"assembly-{name}__live__{n:03d}.json")):
            n += 1
        path = os.path.join(cd.ORIG, f"assembly-{name}__live__{n:03d}.json")
        mine = [b for b in d["bodies"]]
        json.dump({"name": name, "player": r["player"], "location": r["location"], "heading": r["heading"],
                   "comps": r["comps"], "tree": r["tree"], "first": r.get("first"),
                   "bodies": mine}, open(path, "w"), indent=0)
        print("wrote", path, len(r["comps"]), "components")


if __name__ == "__main__":
    main()
