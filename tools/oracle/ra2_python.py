#!/usr/bin/env python3
"""Run Python 2.2 code inside the original Robot Arena 2 and print its output.

Uses tools/oracle/frida/ra2_python.js through the in-sandbox frida-server
(see tools/capture/original_session.sh).  The code runs on the game's main
thread at the start of a UI frame; anything it writes to the file object
``out`` (bound for you) is printed here.

    ra2_python.py -c 'import plus; out.write(repr(plus.getPlayers()))'
    ra2_python.py script.py
    ra2_python.py --sample 'repr([plus.getLocation(p) for p in plus.getPlayers()])' \
        --every 10 --seconds 20 > trace.txt     # one line per sampled frame

Python 2.2 inside the game: no `with`, no f-strings, no True/False builtins
before 2.2.1 (use 1/0), and generators need `from __future__ import generators`.
"""
import argparse
import os
import sys
import time

import frida

HERE = os.path.dirname(os.path.abspath(__file__))
AGENT = os.path.join(HERE, "frida", "ra2_python.js")
DRIVE_C = os.path.expanduser("~/.var/app/org.winehq.Wine/data/wine/drive_c")
OUT_DIR = os.path.join(DRIVE_C, "ra2oracle")
WIN_OUT = r"C:\\ra2oracle\\"

WRAP = """
import sys
out = open('%(path)s', 'w')
try:
    try:
%(body)s
    except:
        import traceback
        traceback.print_exc(None, out)
finally:
    out.close()
"""

SAMPLE = """
import plus
_o = open('%(path)s', 'a')
try:
    try:
        _o.write(repr(plus.getTimeElapsed()) + '\\t' + (%(expr)s) + '\\n')
    except:
        import traceback
        traceback.print_exc(None, _o)
finally:
    _o.close()
"""


def connect(host, name):
    dev = frida.get_device_manager().add_remote_device(host)
    procs = [p for p in dev.enumerate_processes() if p.name == name]
    if not procs:
        sys.exit(f"ra2_python: no process named {name!r} on {host}")
    session = dev.attach(procs[0].pid)
    with open(AGENT) as f:
        script = session.create_script(f.read())
    script.load()
    return session, script.exports_sync


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("file", nargs="?")
    ap.add_argument("-c", dest="code")
    ap.add_argument("--sample", help="Python expression (a str) sampled every N frames")
    ap.add_argument("--every", type=int, default=10)
    ap.add_argument("--seconds", type=float, default=10)
    ap.add_argument("--host", default=os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142"))
    ap.add_argument("--name", default="Robot Arena 2.exe")
    a = ap.parse_args()
    os.makedirs(OUT_DIR, exist_ok=True)
    session, api = connect(a.host, a.name)
    try:
        if a.sample:
            host_path = os.path.join(OUT_DIR, "sample.txt")
            open(host_path, "w").close()
            api.everyframe(SAMPLE % {"path": WIN_OUT + "sample.txt", "expr": a.sample}, a.every)
            time.sleep(a.seconds)
            api.everyframe(None, 1)
            time.sleep(0.2)
            sys.stdout.write(open(host_path).read())
            return
        src = a.code if a.code is not None else open(a.file).read()
        body = "\n".join("        " + line for line in src.splitlines()) or "        pass"
        host_path = os.path.join(OUT_DIR, "out.txt")
        if os.path.exists(host_path):
            os.remove(host_path)
        t = api.exec(WRAP % {"path": WIN_OUT + "out.txt", "body": body})
        deadline = time.time() + 15
        while api.result(t) is None and time.time() < deadline:
            time.sleep(0.05)
        rc = api.result(t)
        if os.path.exists(host_path):
            sys.stdout.write(open(host_path).read())
        if rc != 0:
            sys.exit(f"ra2_python: PyRun_SimpleString returned {rc}")
    finally:
        session.detach()


if __name__ == "__main__":
    main()
