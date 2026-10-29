#!/usr/bin/env python3
"""The floating damage numbers of the ORIGINAL (docs/37 section 7).

Needs `tools/capture/original_session.sh start` and the game at the main menu.  Scout vs ALARM in the
Combat Arena (stock robots, routes/exhibition_box_scout_alarm.steps), hazards switched off by the
caller's arena, both AIs stopped; with frida/ra2_numbers.js loaded it then

  hazard   `plus.damage(1, 0, AMOUNT, point)` (the hazard path, no attacker) and a screenshot series
  hit      the Scout driven into ALARM (attacker hits, the green numbers) with a screenshot series

  NAME=numbers SECS=6 OUT=<dir> STAGES=hazard,hit tools/oracle/capture_numbers.py

Writes OUT/<NAME>__live__<seq>.jsonl (agent rows + shot rows) and ...-<k>.png.  Nothing in the game
directory is touched.
"""
import json
import math
import os
import subprocess
import sys
import threading
import time

import frida

import capture_drive as cd
import ra2_drive
import ra2_python

HERE = os.path.dirname(os.path.abspath(__file__))
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:" + os.environ.get("RA2_FRIDA_PORT", "27142"))
DISPLAY = os.environ.get("RA2_CAPTURE_DISPLAY", ":5")

rows = []
lock = threading.Lock()
XTEST = os.path.join(os.path.dirname(HERE), "capture", "xtest.py")


class Grab:
    """Burst screen grabs (xgrab.py, a system-python helper) into a scratch directory."""

    def __init__(self):
        import tempfile
        self.dir = tempfile.mkdtemp(prefix="numbers-grab-", dir=os.environ.get("GRAB_DIR"))

    def start(self):
        self.proc = subprocess.Popen(["python3", os.path.join(HERE, "xgrab.py"), DISPLAY, self.dir])

    def stop(self):
        self.proc.terminate()
        self.proc.wait()

    def save(self, base, k, why, rows, every=1):
        out = subprocess.run(["python3", os.path.join(HERE, "xgrab.py"), "convert", self.dir, base, str(k), str(every)],
                             capture_output=True, text=True, check=True).stdout
        for line in out.splitlines():
            n, t = line.split()
            with lock:
                rows.append({"ev": "shot", "file": f"{os.path.basename(base)}-{int(n):03d}.png", "why": why, "host_t": float(t)})
            k = int(n) + 1
        print("saved", len(out.splitlines()), "frames", why, flush=True)
        return k


class Py:
    def __init__(self):
        self.session, self.api = ra2_python.connect(HOST, "Robot Arena 2.exe")
        os.makedirs(ra2_python.OUT_DIR, exist_ok=True)
        self.path = os.path.join(ra2_python.OUT_DIR, "numbers_out.txt")

    def run(self, src, timeout=15):
        body = "\n".join("        " + line for line in src.splitlines()) or "        pass"
        if os.path.exists(self.path):
            os.remove(self.path)
        t = self.api.exec(ra2_python.WRAP % {"path": ra2_python.WIN_OUT + "numbers_out.txt", "body": body})
        end = time.time() + timeout
        while self.api.result(t) is None and time.time() < end:
            time.sleep(0.02)
        return open(self.path).read() if os.path.exists(self.path) else ""


def flush(base):
    with lock:
        text = "".join(json.dumps(r, separators=(",", ":")) + "\n" for r in rows)
    with open(base + ".jsonl", "w") as fh:
        fh.write(text)


CAM = """
import plus, math
a = plus.getLocation(%d)
h = plus.getHeading(%d)
fx, fz = math.sin(h), math.cos(h)
rx, rz = fz, -fx
m = (a[0] + %f * fx + %f * rx, a[1] + %f, a[2] + %f * fz + %f * rz)
t = (a[0] + %f * fx, a[1] + %f, a[2] + %f * fz)
d = [t[0] - m[0], t[1] - m[1], t[2] - m[2]]
L = math.sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2])
d = [d[0] / L, d[1] / L, d[2] / L]
plus.setCameraPosition(m[0], m[1], m[2])
plus.setCameraRotation(-math.asin(d[1]), math.atan2(d[0], d[2]))
plus.setCameraFOV(%f)
out.write(repr((m, h, -math.asin(d[1]), math.atan2(d[0], d[2]))))
"""


def camera(py, player, fwd, side, up, look_fwd, fov=0.8):
    return py.run(CAM % (player, player, fwd, side, up, fwd, side, look_fwd, 0.15, look_fwd, fov))


def main():
    name = os.environ.get("NAME", "numbers")
    out_dir = os.environ.get("OUT", cd.ORIG)
    secs = float(os.environ.get("SECS", "6"))
    stages = os.environ.get("STAGES", "hazard,hit").split(",")
    amount = float(os.environ.get("AMOUNT", "25"))
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    n = 0
    while os.path.exists(os.path.join(out_dir, f"{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"{name}__live__{n:03d}")
    os.makedirs(out_dir, exist_ok=True)

    def on_msg(m, d):
        if m.get("type") != "send":
            print("agent error", m, file=sys.stderr)
            return
        p = m["payload"]
        p["host_t"] = time.time()
        with lock:
            rows.append(p)

    sc = sess.create_script(open(os.path.join(HERE, "frida", "ra2_numbers.js")).read())
    sc.on("message", on_msg)
    sc.load()
    cam_sc = sess.create_script(open(os.path.join(HERE, "frida", "ra2_camera.js")).read())
    cam_sc.on("message", lambda m, d: m.get("type") == "send" and on_msg({"type": "send", "payload": dict(m["payload"], ev="cam")}, d))
    cam_sc.load()
    if os.environ.get("MATCH") != "running":
        if os.environ.get("SETUP", "full") == "full":
            cd.drive(["-f", os.path.join(HERE, "routes", "exhibition_box_crush_alarm.steps")])
        cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", "188", "wait", "1", "click", "295", "487", "wait", "0.5", "hide"])  # arena, hazards off
        cd.drive(["click", "936", "647", "hide", "wait", "14", "click", "512", "735", "hide", "wait", "1"])
    py = Py()
    print("neuter:", py.run(cd.NEUTER), flush=True)
    k = 0
    if os.environ.get("KEEP") == "setup":
        py.session.detach()
        sc.unload()
        sess.detach()
        return
    try:
        if "hazard" in stages:
            for ci, camspec in enumerate(os.environ.get("CAM", "1.3,1.9,0.9,1.3").split(";")):
                cam = [float(x) for x in camspec.split(",")]
                print("camera:", camera(py, 0, *cam), flush=True)
                g = Grab()
                g.start()
                time.sleep(0.4)
                t = time.time()
                with lock:
                    rows.append({"ev": "damage", "amount": amount, "cam": cam, "host_t": t})
                print(py.run("import plus\nl = plus.getLocation(0)\nplus.damage(0, 0, %f, (l[0], l[1] + 0.3, l[2]))\nout.write(repr(l))" % amount), flush=True)
                time.sleep(1.8)
                g.stop()
                k = g.save(base, k, "hazard%d" % ci, rows, every=int(os.environ.get("EVERY", "3")))
                flush(base)
        if "hit" in stages:
            # the human robot is driven with the keyboard (C.R.U.S.H. is wired to WASD): W held, A/D
            # tapped to keep ALARM dead ahead
            hold = float(os.environ.get("HOLD", "14"))
            sign = float(os.environ.get("TURN_SIGN", "1"))

            def key(k, d):
                subprocess.run([sys.executable, XTEST, "--display", DISPLAY, "key", k, d], stdout=subprocess.DEVNULL)

            key("w", "down")
            if os.environ.get("ALARM_GO"):
                py.run("import AI\nAI.running_ai[0].Throttle(%s)\nAI.running_ai[0].Turn(0)" % os.environ["ALARM_GO"])
            g = Grab()
            g.start()
            t0 = time.time()
            cur = None
            while time.time() - t0 < hold:
                s = py.run("import plus\nout.write(repr((plus.getLocation(0), plus.getHeading(0), plus.getLocation(1))))")
                try:
                    a0, h, a1 = eval(s)
                except Exception:
                    continue
                err = (math.atan2(a1[0] - a0[0], a1[2] - a0[2]) - h + math.pi) % (2 * math.pi) - math.pi
                with lock:
                    rows.append({"ev": "aim", "err": err, "dist": math.hypot(a1[0] - a0[0], a1[2] - a0[2]), "pose": [a0, h, a1], "host_t": time.time()})
                want = None if (sign == 0 or abs(err) < 0.12) else ("d" if err * sign > 0 else "a")
                if want != cur:
                    if cur:
                        key(cur, "up")
                    if want:
                        key(want, "down")
                    cur = want
            if cur:
                key(cur, "up")
            key("w", "up")
            g.stop()
            k = g.save(base, k, "hit", rows, every=int(os.environ.get("EVERY", "3")))
    finally:
        flush(base)
        sc.unload()
        sess.detach()
        counts = {}
        for r in rows:
            counts[r["ev"]] = counts.get(r["ev"], 0) + 1
        print("wrote", base, counts)
        try:
            if os.environ.get("KEEP"):
                return
            py.run("import plus; plus.eliminatePlayer(1)")
            time.sleep(6)
            cd.drive(["click", "717", "575", "hide", "wait", "3", "click", "527", "745", "hide", "wait", "3"])
        finally:
            py.session.detach()


if __name__ == "__main__":
    main()
