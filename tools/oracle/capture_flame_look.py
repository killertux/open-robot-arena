#!/usr/bin/env python3
"""The Flamethrower of the ORIGINAL, frame by frame and against distance (docs/37 section 6).

Needs `tools/capture/original_session.sh start`.  Installs FLAME_BOT (a `flamebot` robot with FLAME_TANKS tanks)
over the *Wine prefix's* AI/Team0/Bot0.bot (backed up and restored), starts Scout v ALARM in the Combat Arena
(hazards off) with both AIs stopped and the immobility limit raised, and then

  look    the jet fired at level 100 for LOOK_SECS into the open (ALARM 27 m away), a burst of screen grabs
          (xgrab.py, XGRAB_INTERVAL s apart) with the scripted camera of docs/37 section 6, and every emitter
          update (dt, spawn accumulator), spawn (position, velocity, colour, size, life) and draw (all live
          particles) from frida/ra2_flame.js
  range   ALARM's bodies held (frida/ra2_pin.js, every Havok substep) at each gap in RANGES (metres from the
          flame robot's origin to ALARM's origin; 3.4 default list 2.6,3.0,3.4,3.8) and the jet fired for
          RANGE_SECS: ALARM's component health, the `hit` (0x504ca0) and `dmg` (0x4175b0) rows

  FLAME_BOT=<bot> NAME=flamelook OUT=<dir> STAGES=look,range tools/oracle/capture_flame_look.py

Writes OUT/<NAME>__live__<seq>.jsonl (all agent rows + `shot`/`health`/`phase` rows; every row has host_t, the
unix time of the grabs in XGRAB_DIR) and prints the grab directory.
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
import capture_flame as cf
import ra2_drive

HERE = os.path.dirname(os.path.abspath(__file__))
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
DISPLAY = os.environ.get("RA2_CAPTURE_DISPLAY", ":5")
GAME = cf.GAME

rows = []
lock = threading.Lock()


def log(r):
    r["host_t"] = time.time()
    with lock:
        rows.append(r)


class Grab:
    def __init__(self, d):
        self.dir = d
        shutil.rmtree(d, ignore_errors=True)
        os.makedirs(d)

    def start(self):
        env = dict(os.environ, XGRAB_INTERVAL=os.environ.get("XGRAB_INTERVAL", "0.02"))
        self.proc = subprocess.Popen(["python3", os.path.join(HERE, "xgrab.py"), DISPLAY, self.dir], env=env)

    def stop(self):
        self.proc.terminate()
        self.proc.wait()


def load(sess, name, handler=None):
    sc = sess.create_script(open(os.path.join(HERE, "frida", name)).read())
    if handler:
        sc.on("message", handler)
    sc.load()
    return sc


def main():
    name = os.environ.get("NAME", "flamelook")
    out_dir = os.environ.get("OUT", cd.ORIG)
    stages = os.environ.get("STAGES", "look,range").split(",")
    look_secs = float(os.environ.get("LOOK_SECS", "2.5"))
    range_secs = float(os.environ.get("RANGE_SECS", "2.2"))
    ranges = [float(x) for x in os.environ.get("RANGES", "2.6,3.0,3.4,3.8").split(",")]
    grab_dir = os.environ.get("XGRAB_DIR", "/tmp/flamelook-grab")
    bot = os.environ["FLAME_BOT"]
    p0 = os.path.join(GAME, "AI", "Team0", "Bot0.bot")
    shutil.copy2(p0, p0 + ".flame-backup")
    py = None
    grab = Grab(grab_dir)
    try:
        shutil.copy2(bot, p0)
        dev = frida.get_device_manager().add_remote_device(HOST)
        pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
        sess = dev.attach(pid)

        def on_msg(m, d):
            if m.get("type") != "send":
                print("agent error", m, file=sys.stderr)
                return
            log(m["payload"])

        sc = load(sess, "ra2_flame.js", on_msg)
        pin = load(sess, "ra2_pin.js", on_msg)
        cam_sc = load(sess, "ra2_camera.js", lambda m, d: m.get("type") == "send" and log(dict(m["payload"], ev="cam")))
        cd.drive(["-f", os.path.join(HERE, "routes", "exhibition_box_scout_alarm.steps")])
        cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", "188", "wait", "1", "hide"])
        cd.drive(["click", "936", "647", "hide", "wait", "14", "click", "512", "735", "hide", "wait", "1"])
        py = cf.Py()
        print("neuter:", py.run(cd.NEUTER), flush=True)
        print("immobility limit was", pin.exports_sync.no_immobility())
        time.sleep(1.5)

        def pose():
            return eval(py.run("import plus\nout.write(repr((plus.getTimeElapsed(), plus.getLocation(0), plus.getHeading(0),"
                               " plus.getLocation(1), plus.getHeading(1))))"))

        def inp(on, level=100):
            sc.exports_sync.fire(1 if on else 0, int(level))
            log({"ev": "input", "on": on, "level": level})

        def health():
            v = py.run("import plus\nout.write(repr((plus.getTimeElapsed(), plus.getHitpoints(1, 0), "
                       "[round(plus.getHealth(1, c), 3) for c in range(16)], plus.getHitpoints(0, 0))))")
            log({"ev": "health", "v": v})
            return v

        p0v = pose()
        log({"ev": "pose", "pose": p0v})
        t, a, h, b, hb = p0v
        print("camera:", cf.camera(py, 0, 1.3, 1.9, 0.6, 1.3), flush=True)
        # the flame object is learnt from its first call: a no-op fire(0) tells the agent which one it is
        # (the tick hook of 0x504790 sets it too)
        time.sleep(0.5)
        if "look" in stages:
            log({"ev": "phase", "name": "look"})
            hook = None
            if os.environ.get("D3DHOOK"):  # docs/37 section 6: the render state of the jet's draw
                hook = subprocess.Popen([cd.PY, os.path.join(HERE, "ra2_hook.py"), os.path.join(HERE, "frida", "ra2_d3dstate.js"),
                                         "--seconds", "6"], stdout=open(os.environ["D3DHOOK"], "w"))
                time.sleep(2.0)
            grab.start()
            time.sleep(0.4)
            inp(True)
            t0 = time.time()
            time.sleep(look_secs)
            inp(False)
            time.sleep(0.8)
            grab.stop()
            log({"ev": "grabs", "dir": grab_dir, "n": len(os.listdir(grab_dir))})
            if hook:
                hook.wait(timeout=30)
            time.sleep(1.0)
        if "range" in stages:
            # the bodies of ALARM: its chassis (the one nearest plus.getLocation(1)) and every rigid body within 1.6 m
            ch = pin.exports_sync.chassis()
            tgt = min(ch, key=lambda c: math.dist(c[2], b))
            a0 = int(tgt[0], 16)
            cands = pin.exports_sync.scan(hex(a0 - 0x100000), hex(a0 + 0x100000))
            bodies = [tgt] + [c for c in cands if c[0] != tgt[0] and c[1] > 0 and math.dist(c[2], tgt[2]) < 1.6]
            sc_t = min(ch, key=lambda c: math.dist(c[2], a))
            print("ALARM chassis", tgt, "bodies", len(bodies), "flame robot chassis", sc_t, flush=True)
            states = pin.exports_sync.states([x[0] for x in bodies])
            log({"ev": "bodies", "alarm": bodies, "states": states, "scout": sc_t})
            fx, fz = math.sin(h), math.cos(h)
            cs = sc_t[2]
            for g in ranges:
                want = [cs[0] + g * fx, tgt[2][1], cs[2] + g * fz]
                d = [want[0] - tgt[2][0], 0.0, want[2] - tgt[2][2]]
                plan = [[x[0], [s[0][0] + d[0], s[0][1], s[0][2] + d[2]], s[1]] for x, s in zip(bodies, states)]
                pin.exports_sync.pin(plan)
                time.sleep(1.2)
                log({"ev": "phase", "name": "range", "gap": g, "pose": pose()})
                h0 = health()
                grab_k = os.environ.get("RANGE_GRAB") and abs(g - float(os.environ["RANGE_GRAB"])) < 1e-6
                if grab_k:
                    grab.start()
                    time.sleep(0.3)
                inp(True)
                time.sleep(range_secs)
                inp(False)
                if grab_k:
                    time.sleep(0.5)
                    grab.stop()
                    log({"ev": "grabs", "dir": grab_dir, "n": len(os.listdir(grab_dir)), "gap": g})
                time.sleep(0.6)
                h1 = health()
                print("gap", g, "health", h0[:40], "->", h1[:40], flush=True)
                pin.exports_sync.unpin()
                states = pin.exports_sync.states([x[0] for x in bodies])
                # the next placement starts from where ALARM was put
                tgt = [tgt[0], tgt[1], want]
        n = 0
        while os.path.exists(os.path.join(out_dir, f"{name}__live__{n:03d}.jsonl")):
            n += 1
        base = os.path.join(out_dir, f"{name}__live__{n:03d}")
        os.makedirs(out_dir, exist_ok=True)
        with open(base + ".jsonl", "w") as fh:
            for r in rows:
                fh.write(json.dumps(r, separators=(",", ":")) + "\n")
        counts = {}
        for r in rows:
            counts[r["ev"]] = counts.get(r["ev"], 0) + 1
        print("wrote", base, counts)
        sc.unload()
        pin.unload()
        sess.detach()
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
