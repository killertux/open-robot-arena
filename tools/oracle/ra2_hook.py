#!/usr/bin/env python3
"""Load a Frida agent into the original Robot Arena 2 and print its send()s.

    ra2_hook.py tools/oracle/frida/ra2_damage.js --seconds 60 > damage.jsonl

Each send(payload) becomes one JSON line on stdout.  Uses the in-sandbox
frida-server (tools/capture/original_session.sh).  Detaches cleanly on
timeout or Ctrl-C (a client killed mid-RPC can wedge frida-server; if a later
attach hangs, `kill -9 $(pgrep -x frida-server)` and restart it).
"""
import argparse
import json
import os
import sys
import time

import frida


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("agent")
    ap.add_argument("--seconds", type=float, default=30)
    ap.add_argument("--host", default=os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142"))
    ap.add_argument("--name", default="Robot Arena 2.exe")
    a = ap.parse_args()
    dev = frida.get_device_manager().add_remote_device(a.host)
    procs = [p for p in dev.enumerate_processes() if p.name == a.name]
    if not procs:
        sys.exit(f"ra2_hook: no process named {a.name!r}")
    session = dev.attach(procs[0].pid)

    def on_message(msg, data):
        if msg.get("type") == "send":
            print(json.dumps(msg["payload"]), flush=True)
        else:
            print(json.dumps({"agent_error": msg}), file=sys.stderr, flush=True)

    with open(a.agent) as f:
        script = session.create_script(f.read())
    script.on("message", on_message)
    script.load()
    try:
        time.sleep(a.seconds)
    except KeyboardInterrupt:
        pass
    finally:
        script.unload()
        session.detach()


if __name__ == "__main__":
    main()
