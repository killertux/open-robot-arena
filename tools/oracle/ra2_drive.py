#!/usr/bin/env python3
"""Drive the original Robot Arena 2 (under flatpak Wine) through Frida.

Needs the frida Python package (``~/tools/frida-venv/bin/python3``) and a
frida-server running *inside* the Wine flatpak sandbox (see
``tools/capture/original_session.sh``; host-side ``frida -p`` fails with
``unable to perform ptrace pokedata: Input/output error`` because the injector
resolves the sandbox's libc against the host's).

Steps are given on the command line or one per line in a file (``-f``):

    click X Y [BUTTON] [HOLD]   UI click at game pixels (BUTTON 0=left)
    drag X0 Y0 X1 Y1 [BUTTON] [STEPS]   press, move over STEPS frames, release
    move X Y                    pin the UI cursor at X Y
    hide                        park the cursor off-screen (for fixtures)
    wait SECONDS
    frames N                    wait for N UI frames
    shot PATH                   capture the X root window to PATH (.png)
    key KEYSYM                  XTEST key press (the dinput keyboard does see it)
    eval JS                     evaluate JS in the agent (prints the result)

Example:

    ra2_drive.py --display :5 click 514 346 wait 2 shot /tmp/exhibition.png
"""
import argparse
import os
import shlex
import subprocess
import sys
import time

import frida

HERE = os.path.dirname(os.path.abspath(__file__))
AGENT = os.path.join(HERE, "frida", "ra2_input.js")
XWD2PNG = os.path.join(HERE, "..", "capture", "xwd2png.py")
XTEST = os.path.join(HERE, "..", "capture", "xtest.py")


def connect(host, name):
    dev = frida.get_device_manager().add_remote_device(host)
    procs = [p for p in dev.enumerate_processes() if p.name == name]
    if not procs:
        sys.exit(f"ra2_drive: no process named {name!r} on {host}")
    session = dev.attach(procs[0].pid)
    with open(AGENT) as f:
        script = session.create_script(f.read() + EVAL_EXPORT)
    script.on("message", lambda m, d: print("agent:", m, file=sys.stderr))
    script.load()
    return session, script.exports_sync


EVAL_EXPORT = """
rpc.exports.evaljs = function (src) { return String(eval(src)); };
"""


def shot(display, path):
    xwd = path + ".xwd"
    subprocess.run(["xwd", "-root", "-silent", "-out", xwd],
                   env=dict(os.environ, DISPLAY=display), check=True)
    subprocess.run(["python3", XWD2PNG, xwd, path, "--quiet"],
                   check=True, stdout=subprocess.DEVNULL)
    os.remove(xwd)
    print("shot", path)


def wait_frames(api, n):
    start = api.frames()
    deadline = time.time() + 10
    while api.frames() - start < n and time.time() < deadline:
        time.sleep(0.1)


def run(api, display, steps):
    i = 0
    while i < len(steps):
        op = steps[i]; i += 1
        def arg():
            nonlocal i
            v = steps[i]; i += 1
            return v
        if op == "click":
            x, y = int(arg()), int(arg())
            b, hold = 0, 3
            if i < len(steps) and steps[i].isdigit():
                b = int(arg())
                if i < len(steps) and steps[i].isdigit():
                    hold = int(arg())
            api.click(x, y, b, hold)
            wait_frames(api, hold + 4)
        elif op == "drag":
            x0, y0, x1, y1 = (int(arg()) for _ in range(4))
            b, n = 0, 10
            if i < len(steps) and steps[i].isdigit():
                b = int(arg())
                if i < len(steps) and steps[i].isdigit():
                    n = int(arg())
            api.drag(x0, y0, x1, y1, b, n)
            wait_frames(api, n + 6)
        elif op == "move":
            api.setpos(int(arg()), int(arg())); wait_frames(api, 2)
        elif op == "hide":
            api.setpos(4000, 4000); wait_frames(api, 2)
        elif op == "wait":
            time.sleep(float(arg()))
        elif op == "frames":
            wait_frames(api, int(arg()))
        elif op == "shot":
            shot(display, arg())
        elif op == "key":
            subprocess.run(["python3", XTEST, "key", arg()],
                           env=dict(os.environ, DISPLAY=display), check=True,
                           stdout=subprocess.DEVNULL)
        elif op == "eval":
            print(api.evaljs(arg()))
        else:
            sys.exit(f"ra2_drive: unknown step {op!r}")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--display", default=os.environ.get("RA2_CAPTURE_DISPLAY", ":5"))
    ap.add_argument("--host", default=os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142"))
    ap.add_argument("--name", default="Robot Arena 2.exe")
    ap.add_argument("-f", "--file", help="read steps from a file (# comments)")
    ap.add_argument("steps", nargs="*")
    a = ap.parse_args()
    steps = list(a.steps)
    if a.file:
        with open(a.file) as f:
            for line in f:
                line = line.split("#", 1)[0].strip()
                if line:
                    steps += shlex.split(line)
    session, api = connect(a.host, a.name)
    try:
        run(api, a.display, steps)
    finally:
        session.detach()


if __name__ == "__main__":
    main()
