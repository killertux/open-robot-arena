#!/usr/bin/env python3
"""The Flamethrower of the ORIGINAL (docs/37 section 6).

Needs `tools/capture/original_session.sh start` and the game at the main menu.
Installs a test robot (tools: `ra2sim --example flamebot`, a Scout with a Flamethrower on
its nose and two wires, analog `Flame` = "Fire", button `FlameBurst` = "Fire burst") over the
*Wine prefix's* AI/Team0/Bot0.bot (backed up and restored; local/game is never touched), runs
Scout vs ALARM in the Combat Arena without hazards, stops both AIs and then:

  open   the jet fired into the open, level 100 / 50 / 20 and one burst (frida/ra2_flame.js
         ticks, a screenshot series with a scripted camera)
  hit    Scout turned on ALARM and driven to FLAME_RANGE metres, jet at level 100 for SECS
         seconds, ALARM's component health sampled while it burns

  FLAME_BOT=<bot> NAME=flame SECS=8 OUT=<dir> tools/oracle/capture_flame.py

Writes OUT/<NAME>__live__<seq>.jsonl (agent rows + `shot` + `health` rows) and ...-<k>.png.
"""
import json
import math
import os
import shutil
import subprocess
import sys
import threading
import time

import frida

import capture_drive as cd
import ra2_drive
import ra2_python

HERE = os.path.dirname(os.path.abspath(__file__))
GAME = os.path.expanduser("~/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2")
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
DISPLAY = os.environ.get("RA2_CAPTURE_DISPLAY", ":5")

rows = []
lock = threading.Lock()


class Py:
    """A long-lived in-game Python connection."""

    def __init__(self):
        self.session, self.api = ra2_python.connect(HOST, "Robot Arena 2.exe")
        os.makedirs(ra2_python.OUT_DIR, exist_ok=True)
        self.path = os.path.join(ra2_python.OUT_DIR, "flame_out.txt")

    def run(self, src, timeout=15):
        body = "\n".join("        " + line for line in src.splitlines()) or "        pass"
        if os.path.exists(self.path):
            os.remove(self.path)
        t = self.api.exec(ra2_python.WRAP % {"path": ra2_python.WIN_OUT + "flame_out.txt", "body": body})
        end = time.time() + timeout
        while self.api.result(t) is None and time.time() < end:
            time.sleep(0.02)
        return open(self.path).read() if os.path.exists(self.path) else ""


def flush(base):
    with lock:
        text = "".join(json.dumps(r, separators=(",", ":")) + "\n" for r in rows)
    with open(base + ".jsonl", "w") as fh:
        fh.write(text)


def shot(base, k, why):
    path = f"{base}-{k:02d}.png"
    ra2_drive.shot(DISPLAY, path)
    with lock:
        rows.append({"ev": "shot", "file": os.path.basename(path), "why": why, "host_t": time.time()})


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
out.write(repr((m, h)))
"""


def camera(py, player, fwd, side, up, look_fwd, fov=0.8):
    # camera `fwd` metres ahead of the robot, `side` to its right, `up` above, looking at the point
    # `look_fwd` metres ahead of the robot
    return py.run(CAM % (player, player, fwd, side, up, fwd, side, look_fwd, 0.15, look_fwd, fov))


def main():
    name = os.environ.get("NAME", "flame")
    out_dir = os.environ.get("OUT", cd.ORIG)
    secs = float(os.environ.get("SECS", "8"))
    rng = float(os.environ.get("FLAME_RANGE", "1.0"))
    bot = os.environ["FLAME_BOT"]
    p0 = os.path.join(GAME, "AI", "Team0", "Bot0.bot")
    shutil.copy2(p0, p0 + ".flame-backup")
    py = None
    try:
        shutil.copy2(bot, p0)
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

        sc = sess.create_script(open(os.path.join(HERE, "frida", "ra2_flame.js")).read())
        sc.on("message", on_msg)
        sc.load()
        cam_sc = sess.create_script(open(os.path.join(HERE, "frida", "ra2_camera.js")).read())
        cam_sc.on("message", lambda m, d: m.get("type") == "send" and on_msg({"type": "send", "payload": dict(m["payload"], ev="cam")}, d))
        cam_sc.load()
        cd.drive(["-f", os.path.join(HERE, "routes", "exhibition_box_scout_alarm.steps")])
        cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", "188", "wait", "1", "hide"])
        cd.drive(["click", "936", "647", "hide", "wait", "14", "click", "512", "735", "hide", "wait", "1"])
        py = Py()
        if "wait" in os.environ.get("STAGES", "open,hit").split(","):
            # only the flame robot is stopped; ALARM's own AI brings itself into the jet
            print("neuter:", py.run("import AI, plus\ndef _nop(self):\n    pass\nAI.running_ai[0].__class__.Think = _nop\n"
                                    "AI.running_ai[0].tactics = []\nAI.running_ai[0].Throttle(0)\nAI.running_ai[0].Turn(0)\n"
                                    "out.write(repr(plus.getTimeElapsed()))"), flush=True)
        else:
            print("neuter:", py.run(cd.NEUTER + "\nimport AI\nAI.running_ai[0].Throttle(40)"), flush=True)
        k = 0

        def inp(ctl, v):
            if ctl == "Flame":
                sc.exports_sync.fire(1 if v else 0, int(v))
            else:
                sc.exports_sync.burst(1 if v else 0, 100)
            with lock:
                rows.append({"ev": "input", "ctl": ctl, "v": v, "host_t": time.time()})

        def pose():
            s = py.run("import plus\nout.write(repr((plus.getTimeElapsed(), plus.getLocation(0), plus.getHeading(0),"
                       " plus.getLocation(1), plus.getHeading(1))))")
            return eval(s)

        p0v = pose()
        with lock:
            rows.append({"ev": "pose", "pose": p0v, "host_t": time.time()})
        # open: a jet into the open, from the side
        stage = os.environ.get("STAGES", "open,hit").split(",")
        if "open" in stage:
            # the match counts a robot out after 20 s without moving 1.4 m: creep along
            py.run("import AI\nAI.running_ai[0].Throttle(8)\nAI.running_ai[0].Turn(0)")
            print("camera:", camera(py, 0, 1.3, 1.9, 0.6, 1.3))
            shot(base, k, "idle"); k += 1
            for level, hold, nshots in ((100, 2.4, 4), (50, 1.6, 3), (20, 1.6, 2)):
                inp("Flame", level)
                t0 = time.time()
                for i in range(nshots):
                    camera(py, 0, 1.3, 1.9, 0.6, 1.3)
                    sp = pose()
                    with lock:
                        rows.append({"ev": "shotpose", "pose": sp, "k": k, "host_t": time.time()})
                    shot(base, k, f"level{level}"); k += 1
                time.sleep(max(0, t0 + hold - time.time()))
                inp("Flame", 0)
                time.sleep(0.6)
                flush(base)
            inp("FlameBurst", 100)
            time.sleep(0.04)
            camera(py, 0, 1.3, 1.9, 0.6, 1.3)
            shot(base, k, "burst"); k += 1
            inp("FlameBurst", 0)
            time.sleep(0.6)
            py.run("import AI\nAI.running_ai[0].Throttle(0)")
            flush(base)
        if "wait" in stage:
            # fire as soon as ALARM is within FLAME_RANGE metres and keep firing for SECS seconds
            health = ("import plus\nout.write(repr((plus.getTimeElapsed(), plus.getHitpoints(1, 0), "
                      "[round(plus.getHealth(1, c), 3) for c in range(16)], plus.getHitpoints(0, 0), "
                      "plus.getLocation(0), plus.getHeading(0), plus.getLocation(1), plus.getHeading(1))))")
            print("camera:", camera(py, 0, 1.3, 1.9, 0.6, 1.3), flush=True)
            started = None
            i = 0
            t_end = time.time() + 60
            while time.time() < t_end:
                v = py.run(health)
                with lock:
                    rows.append({"ev": "health", "v": v, "host_t": time.time()})
                try:
                    r = eval(v)
                    a, b = r[4], r[6]
                    dist = math.hypot(b[0] - a[0], b[2] - a[2])
                except Exception:
                    dist = 99
                if started is None:
                    # ALARM's own AI is neutered for the session (a class patch): drive it in
                    thr = 80 if dist > 10 else 30 if dist > 5 else 12 if dist > rng + 0.3 else 0
                    py.run("import AI\nAI.running_ai[1].Throttle(%d)" % thr)
                if started is None and dist < rng:
                    py.run("import AI\nAI.running_ai[1].Throttle(0)")
                    inp("Flame", 100)
                    started = time.time()
                    shot(base, k, "wait-start"); k += 1
                if started is not None:
                    if i % 12 == 0:
                        camera(py, 0, 1.3, 1.9, 0.6, 1.3)
                        shot(base, k, "wait"); k += 1
                    if time.time() - started > secs:
                        break
                i += 1
                flush(base)
            inp("Flame", 0)
            time.sleep(1.5)
            hv = py.run(health)
            with lock:
                rows.append({"ev": "health", "v": hv, "host_t": time.time()})
        if "hit" in stage:
            # close in on ALARM (the match spawns the robots facing each other), slowly for
            # the last metres so the jet starts at FLAME_RANGE
            sign = float(os.environ.get("TURN_SIGN", "1"))
            last = None
            for it in range(200):
                t, a, h, b, hb = pose()
                dx, dz = b[0] - a[0], b[2] - a[2]
                dist = math.hypot(dx, dz)
                want = math.atan2(dx, dz)
                err = (want - h + math.pi) % (2 * math.pi) - math.pi
                with lock:
                    rows.append({"ev": "aim", "t": t, "dist": dist, "err": err, "host_t": time.time()})
                if dist <= rng:
                    break
                if last is not None and abs(err) > last + 0.03:
                    sign = -sign
                last = abs(err)
                turn = max(-100, min(100, err * 120)) * sign
                thr = 0 if abs(err) > 0.7 else (70 if dist > 8 else 30 if dist > 3 else 14)
                py.run("import AI\nAI.running_ai[0].Turn(%d)\nAI.running_ai[0].Throttle(%d)" % (turn, thr))
            py.run("import AI\nAI.running_ai[0].Turn(0)\nAI.running_ai[0].Throttle(0)")
            time.sleep(0.5)
            p1v = pose()
            with lock:
                rows.append({"ev": "pose", "pose": p1v, "host_t": time.time()})
            print("camera:", camera(py, 0, 1.4, 1.5, 0.9, 1.0))
            health = ("import plus\nout.write(repr((plus.getTimeElapsed(), plus.getHitpoints(1, 0), "
                      "[round(plus.getHealth(1, c), 3) for c in range(16)], plus.getHitpoints(0, 0))))")
            inp("Flame", 100)
            t0 = time.time()
            i = 0
            while time.time() - t0 < secs:
                s = py.run(health)
                with lock:
                    rows.append({"ev": "health", "v": s, "host_t": time.time()})
                if i in (3, 40):
                    shot(base, k, "hit"); k += 1
                i += 1
                flush(base)
            inp("Flame", 0)
            time.sleep(1.5)
            hv = py.run(health)
            with lock:
                rows.append({"ev": "health", "v": hv, "host_t": time.time()})
        sc.unload()
        sess.detach()
        with open(base + ".jsonl", "w") as fh:
            for r in rows:
                fh.write(json.dumps(r, separators=(",", ":")) + "\n")
        counts = {}
        for r in rows:
            counts[r["ev"]] = counts.get(r["ev"], 0) + 1
        print("wrote", base, counts)
        py.run("import plus; plus.eliminatePlayer(1)")
        time.sleep(6)
        cd.drive(["click", "717", "575", "hide", "wait", "3", "click", "527", "745", "hide", "wait", "3"])
    finally:
        if py:
            py.session.detach()
        if os.path.exists(p0 + ".flame-backup"):
            shutil.move(p0 + ".flame-backup", p0)


if __name__ == "__main__":
    main()
