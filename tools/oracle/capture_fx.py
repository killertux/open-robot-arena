#!/usr/bin/env python3
"""Per-hit visual effects of the ORIGINAL during an AI-vs-AI fight (docs/37-effects.md).

Needs `tools/capture/original_session.sh start` and the game at the main menu.
Copies two stock robots over the *Wine prefix's* AI/Team0/Bot0.bot and Bot1.bot
(backed up and restored; local/game is never touched), sets the exhibition up
(routes/exhibition_box_scout_alarm.steps), clears the arena's "Hazards enabled"
box, starts the match, skips the intro and lets both AIs fight for SECS wall
seconds with frida/ra2_fx.js loaded.  A screenshot of the root window is taken
shortly after a decal / spark / smoke event (at most one per SHOT_GAP seconds).

  BOT0=AI/Team14/Bot2.bot BOT1=AI/Team13/Bot1.bot NAME=axe-v-hammer SECS=60 \
      OUT=<dir> tools/oracle/capture_fx.py

Writes OUT/fx-<NAME>__live__<seq>.jsonl (events) and ...-<k>.png (shots).
"""
import json
import os
import shutil
import subprocess
import sys
import threading
import time

import frida

import capture_drive as cd
import ra2_drive

HERE = os.path.dirname(os.path.abspath(__file__))
GAME = os.path.expanduser("~/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2")
SRC = os.environ.get("RA2_GAME_SRC", os.path.join(os.path.dirname(os.path.dirname(HERE)), "local", "game"))
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
DISPLAY = os.environ.get("RA2_CAPTURE_DISPLAY", ":5")


POSE_CODE = """
import plus, math
a = plus.getLocation(%d)
out.write(repr((a, plus.getHeading(%d), plus.isUpsideDown(%d))))
"""
CAM_CODE = """
import plus, math
a = plus.getLocation(%d)
m = (a[0] + %f, a[1] + %f, a[2] + %f)
t = (a[0], a[1] + %f, a[2])
d = [t[0] - m[0], t[1] - m[1], t[2] - m[2]]
L = math.sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2])
d = [d[0] / L, d[1] / L, d[2] / L]
yaw = math.atan2(d[0], d[2])
pitch = -math.asin(d[1])
plus.setCameraPosition(m[0], m[1], m[2])
plus.setCameraRotation(pitch, yaw)
plus.setCameraFOV(%f)
out.write(repr((m, pitch, yaw)))
"""


def pose_shots(base, rows, sc):
    """Freeze both robots (AI neutered), then photograph each from four sides with a
    scripted camera.  Logged as `pose` rows: the robot's location and heading, the
    camera's position, pitch, yaw and FOV, and the file of the shot."""
    print("neuter:", cd.py(cd.NEUTER))
    time.sleep(float(os.environ.get("SETTLE", "7.0")))
    subprocess.run(["python3", os.path.join(HERE, "..", "capture", "xtest.py"), "key", "F3"],
                   env=dict(os.environ, DISPLAY=DISPLAY), check=False)
    time.sleep(1.0)
    dist = float(os.environ.get("POSE_DIST", "1.9"))
    fov = float(os.environ.get("POSE_FOV", "0.7"))
    k = 0
    addrs = sorted({r["addr"] for r in rows if r.get("ev") == "inv" and r.get("addr")})
    for player in (0, 1):
        pose = cd.py(POSE_CODE % (player, player, player)).strip()
        for az in range(4):
            import math
            dx, dz = dist * math.cos(az * math.pi / 2), dist * math.sin(az * math.pi / 2)
            cam = cd.py(CAM_CODE % (player, dx, 0.9, dz, 0.1, fov)).strip()
            time.sleep(0.25)
            path = f"{base}-pose{k:02d}.png"
            # the robots creep a few cm a second after the AI is stopped: read the
            # matrices an instant before the frame is grabbed
            mats = {a: sc.exports_sync.m12at(a) for a in addrs}
            ra2_drive.shot(DISPLAY, path)
            rows.append({"ev": "pose", "player": player, "az": az, "pose": pose, "cam": cam, "fov": fov,
                         "chassis": mats, "file": os.path.basename(path)})
            k += 1


def main():
    name = os.environ.get("NAME", "fx")
    out_dir = os.environ.get("OUT", cd.ORIG)
    secs = float(os.environ.get("SECS", "60"))
    gap = float(os.environ.get("SHOT_GAP", "1.2"))
    delay = float(os.environ.get("SHOT_DELAY", "0.12"))
    trig = set(os.environ.get("SHOT_ON", "decal,spark").split(","))
    t0dir = os.path.join(GAME, "AI", "Team0")
    saved = []
    for i in (0, 1):
        p = os.path.join(t0dir, f"Bot{i}.bot")
        shutil.copy2(p, p + ".fx-backup")
        saved.append(p)
    try:
        for i in (0, 1):
            src = os.environ.get(f"BOT{i}")
            if src:
                shutil.copy2(os.path.join(SRC, src), saved[i])
        cd.drive(["-f", os.path.join(HERE, "routes", "exhibition_box_scout_alarm.steps")])
        shot0 = "/tmp/fx-setup.png"
        cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", "188", "wait", "1", "hide", "shot", shot0])
        from compare import read_png
        w, h, rgb = read_png(shot0)
        v = [rgb[3 * (y * w + x) + c] for y in range(484, 493) for x in range(291, 300) for c in range(3)]
        if sum(v) / len(v) > 130 and not os.environ.get("HAZARDS"):
            cd.drive(["click", "295", "488", "wait", "1", "hide"])
        cd.drive(["click", "936", "647", "hide", "wait", "14", "click", "512", "735", "hide", "wait", "1"])
        time.sleep(6)
        cam = os.environ.get("CAM", "F2")
        if cam:
            subprocess.run(["python3", os.path.join(HERE, "..", "capture", "xtest.py"), "key", cam],
                           env=dict(os.environ, DISPLAY=DISPLAY), check=False)
            time.sleep(1)
        dev = frida.get_device_manager().add_remote_device(HOST)
        pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
        sess = dev.attach(pid)
        rows = []
        n = 0
        while os.path.exists(os.path.join(out_dir, f"fx-{name}__live__{n:03d}.jsonl")):
            n += 1
        base = os.path.join(out_dir, f"fx-{name}__live__{n:03d}")
        last = [0.0]
        shots = [0]
        lock = threading.Lock()

        def take(k, ev):
            time.sleep(delay)
            path = f"{base}-{k:02d}.png"
            try:
                ra2_drive.shot(DISPLAY, path)
                rows.append({"ev": "shot", "file": os.path.basename(path), "after": ev})
            except Exception as e:  # noqa: BLE001
                print("shot failed", e, file=sys.stderr)

        def on_msg(m, d):
            if m.get("type") != "send":
                print("agent error", m, file=sys.stderr)
                return
            p = m["payload"]
            p["host_t"] = time.time()
            rows.append(p)
            if p.get("ev") in trig and os.environ.get("NOSHOT") != "1":
                with lock:
                    if time.time() - last[0] >= gap and shots[0] < int(os.environ.get("MAX_SHOTS", "40")):
                        last[0] = time.time()
                        shots[0] += 1
                        threading.Thread(target=take, args=(shots[0], p["ev"]), daemon=True).start()

        sc = sess.create_script(open(os.path.join(HERE, "frida", "ra2_fx.js")).read())
        sc.on("message", on_msg)
        sc.load()
        try:
            print('global 0x71abcc', sc.exports_sync.glob())
        except Exception as e:  # noqa: BLE001
            print('global read failed', e)
        t0 = time.time()
        while time.time() - t0 < secs:
            time.sleep(0.5)
        if os.environ.get("POSE") == "1":
            pose_shots(base, rows, sc)
        sc.unload()
        sess.detach()
        os.makedirs(out_dir, exist_ok=True)
        with open(base + ".jsonl", "w") as fh:
            for r in rows:
                fh.write(json.dumps(r, separators=(",", ":")) + "\n")
        counts = {}
        for r in rows:
            counts[r["ev"]] = counts.get(r["ev"], 0) + 1
        print("wrote", base, counts)
        # end the match (eliminate player 1), then dismiss the results screen
        cd.py("import plus; plus.eliminatePlayer(1)")
        time.sleep(6)
        cd.drive(["click", "717", "575", "hide", "wait", "3", "click", "527", "745", "hide", "wait", "3"])
    finally:
        for p in saved:
            if os.path.exists(p + ".fx-backup"):
                shutil.move(p + ".fx-backup", p)


if __name__ == "__main__":
    main()
