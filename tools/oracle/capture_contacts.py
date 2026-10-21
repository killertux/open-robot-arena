#!/usr/bin/env python3
"""The contact points Havok hands its resolver in the ORIGINAL, during a drive test.

Needs `tools/capture/original_session.sh start` and the exhibition set up as for
capture_drive.py (routes/exhibition_box_scout_alarm.steps).  Neuters the AI,
loads frida/ra2_contacts.js and drives robot PLAYER (env, default 0 = Scout;
1 = ALARM): turn 3 s, coast 1.5 s, aim at the longest free line, straight
STRAIGHT s (default 2.1), stop 3 s.  PLAN=straight drives straight from where
the robot stands (its spawn pose in a fresh match: the `fidtrace drive-at`
tests) after 1.5 s at rest.  The match is ended afterwards (the other robot
eliminated) and the results dismissed, so the next capture starts a new one.  Writes into OUT (default
local/oracle/original):
  contacts-<bot>__live__<seq>.jsonl   raw rows (see the agent's header)
  contacts-<bot>__live__<seq>.json    marks
Set SKIP_START=1 when the match is already running.
`tools/oracle/contacts_report.py` summarises a capture.
"""
import json
import os
import sys
import time

import frida

import capture_drive as cd
import capture_magic as cm

AGENT = os.environ.get("AGENT", os.path.join(cd.HERE, "frida", "ra2_contacts.js"))
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


def main():
    player = int(os.environ.get("PLAYER", "0"))
    name = os.environ.get("BOT", ["scout", "alarm"][player] if player < 2 else f"p{player}")
    out_dir = os.environ.get("OUT", cd.ORIG)
    if os.environ.get("SKIP_START") != "1":
        cd.start_match(os.environ.get("ARENA_ROW_Y", "188"))
    print("neuter:", cd.py(cd.NEUTER))
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
    n = 0
    while os.path.exists(os.path.join(out_dir, f"contacts-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"contacts-{name}__live__{n:03d}")
    time.sleep(1.0)
    t0 = time.time()
    ai = f"import AI\nAI.running_ai[{player}]"
    straight = ("straight", f"{ai}.Throttle(100)\nAI.running_ai[{player}].Turn(0)",
                float(os.environ.get("STRAIGHT", "2.1")))
    stop = ("stop", f"{ai}.Throttle(0)\nAI.running_ai[{player}].Turn(0)", 3.0)
    plan = [("rest", f"{ai}.Throttle(0)\nAI.running_ai[{player}].Turn(0)", 1.5),
            ("turn", f"{ai}.Throttle(0)\nAI.running_ai[{player}].Turn(100)", 3.0),
            ("coast", f"{ai}.Throttle(0)\nAI.running_ai[{player}].Turn(0)", 1.5),
            straight, stop]
    if os.environ.get("PLAN") == "straight":
        plan = [plan[0], straight, stop]
    marks = []
    for mark, code, hold in plan:
        if mark == "straight" and os.environ.get("AIM", "1") == "1" and len(plan) > 3:
            cm.aim(player)
        t = time.time()
        cd.py(code)
        marks.append((mark, t - t0))
        time.sleep(max(0, hold - (time.time() - t)))
    sc.exports_sync.flush()
    time.sleep(0.5)
    sc.unload()
    sess.detach()
    with open(base + ".jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, separators=(",", ":")) + "\n")
    json.dump({"player": player, "marks": marks, "rows": len(rows)}, open(base + ".json", "w"), indent=1)
    print("wrote", base, len(rows), "rows", marks)
    if player < 2:
        cd.py("import plus; plus.eliminatePlayer(%d)" % (1 - player))
        time.sleep(6)
        cd.drive(["click", "717", "575", "hide", "wait", "3"])


if __name__ == "__main__":
    main()
