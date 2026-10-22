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


EYE_CODE = """
import plus, math
m = (%f, %f, %f)
t = (%f, %f, %f)
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


def decal_shots(base, rows, sc, n, tag, forced=None):
    """Photograph the `n` strongest live decals head-on, `DECAL_DIST` m off the face: the
    camera sits along the decal's world normal, looking at its centre.  `forced` writes that
    alpha into the live decals first (ALPHA_FORCE), so the same shot is taken twice, as built
    and at full strength.  Logged as `dshot` rows (the camera and the chassis matrix)."""
    dist = float(os.environ.get("DECAL_DIST", "0.9"))
    fov = float(os.environ.get("POSE_FOV", "0.7"))
    decals = [r for r in rows if r.get("ev") == "decal"][-25:]
    addrs = sorted({r["addr"] for r in rows if r.get("ev") == "inv" and r.get("addr")})
    if forced is not None:
        print("alpha forced on", sc.exports_sync.setalpha([r["obj"] for r in decals], forced), "decals")
    # the strongest decals whose camera stays inside the arena (a robot pinned at a wall has none)
    inside = {}
    for k, r in enumerate(decals):
        owner = next((x["addr"] for x in reversed(rows[:rows.index(r)]) if x.get("ev") == "inv"), addrs[0])
        m = sc.exports_sync.m12at(owner)
        ax = [m[0:3], m[3:6], m[6:9]]
        world = lambda v: [sum(v[j] * ax[j][i] for j in range(3)) for i in range(3)]
        c = [a + b for a, b in zip(world(r["c"]), m[9:12])]
        nw = world(r["u"])
        ln = math.sqrt(sum(x * x for x in nw)) or 1.0
        e = [c[i] + nw[i] / ln * dist for i in range(3)]
        inside[k] = abs(e[0]) < 11.0 and abs(e[2]) < 11.0 and e[1] > -2.6
    best = sorted((i for i in range(len(decals)) if inside[i]), key=lambda i: -decals[i]["alpha"])[:n]
    print("decal shots:", sorted(best), "of", len(decals), "decals")
    for k in sorted(best):
        r = decals[k]
        mats = {a: sc.exports_sync.m12at(a) for a in addrs}
        # the decal's chassis is the one whose matrix is on its hit chain (`inv` row before it)
        owner = next((x["addr"] for x in reversed(rows[:rows.index(r)]) if x.get("ev") == "inv"), addrs[0])
        m = mats[owner]
        ax = [m[0:3], m[3:6], m[6:9]]
        world = lambda v: [sum(v[j] * ax[j][i] for j in range(3)) for i in range(3)]
        c = [a + b for a, b in zip(world(r["c"]), m[9:12])]
        nw = world(r["u"])
        ln = math.sqrt(sum(x * x for x in nw)) or 1.0
        eye = [c[i] + nw[i] / ln * dist for i in range(3)]
        eye[1] += 0.05
        cam = cd.py(EYE_CODE % (*eye, *c, fov)).strip()
        time.sleep(0.4)
        mats = {a: sc.exports_sync.m12at(a) for a in addrs}
        path = f"{base}-{tag}{k:02d}.png"
        ra2_drive.shot(DISPLAY, path)
        rows.append({"ev": "dshot", "k": k, "tag": tag, "alpha": r["alpha"], "cam": cam, "fov": fov,
                     "chassis": mats, "file": os.path.basename(path)})


def decal_pairs(base, rows, sc, n):
    """DECAL_PAIRS=n: for each of the `n` strongest decals in the open, two shots of the same frame
    0.35 s apart from the same camera, one as built and one with that decal forced to alpha 1.0
    (restored afterwards), so the difference of the pair is the decal alone.  `dpair` rows."""
    dist = float(os.environ.get("DECAL_DIST", "0.9"))
    fov = float(os.environ.get("POSE_FOV", "0.7"))
    decals = [r for r in rows if r.get("ev") == "decal"][-25:]
    addrs = sorted({r["addr"] for r in rows if r.get("ev") == "inv" and r.get("addr")})
    best = []
    for k, r in enumerate(decals):
        owner = next((x["addr"] for x in reversed(rows[:rows.index(r)]) if x.get("ev") == "inv"), addrs[0])
        m = sc.exports_sync.m12at(owner)
        ax = [m[0:3], m[3:6], m[6:9]]
        world = lambda v: [sum(v[j] * ax[j][i] for j in range(3)) for i in range(3)]
        c = [a + b for a, b in zip(world(r["c"]), m[9:12])]
        nw = world(r["u"])
        ln = math.sqrt(sum(x * x for x in nw)) or 1.0
        e = [c[i] + nw[i] / ln * dist for i in range(3)]
        if abs(e[0]) < 11.0 and abs(e[2]) < 11.0 and e[1] > -2.6 and nw[1] / ln > -0.5:
            best.append((r["alpha"], k, owner, c, nw, ln))
    best.sort(key=lambda t: -t[0])
    for alpha, k, owner, c, nw, ln in best[:n]:
        r = decals[k]
        eye = [c[i] + nw[i] / ln * dist for i in range(3)]
        eye[1] += 0.05
        cam = cd.py(EYE_CODE % (*eye, *c, fov)).strip()
        time.sleep(0.3)
        mats = {a: sc.exports_sync.m12at(a) for a in addrs}
        pa = f"{base}-pa{k:02d}.png"
        ra2_drive.shot(DISPLAY, pa)
        sc.exports_sync.setalpha([r["obj"]], 1.0)
        time.sleep(0.15)
        mats2 = {a: sc.exports_sync.m12at(a) for a in addrs}
        pb = f"{base}-pb{k:02d}.png"
        ra2_drive.shot(DISPLAY, pb)
        sc.exports_sync.setalpha([r["obj"]], r["alpha"])
        rows.append({"ev": "dpair", "k": k, "alpha": r["alpha"], "cam": cam, "fov": fov, "chassis": mats,
                     "chassis_b": mats2, "a": os.path.basename(pa), "b": os.path.basename(pb)})
        print("pair", k, "alpha", r["alpha"], "normal y", nw[1] / ln, flush=True)


def synth_decals(base, rows, sc, damages, offsets):
    """SYNTH=1 (docs/37 section 8): decals placed on purpose.  After the fight both robots are held at their
    poses (frida/ra2_pin.js) and, for each damage in `damages`, the component-hit routine is run again on the
    most-struck part with the last real contact record, the point moved onto the held robot's flank (the local
    centre and normal of one of its real decals, shifted `offsets[i]` m along the decal's height axis) and the
    normal into the part.  A shot from `DECAL_DIST` m off the face is taken before and after each hit and
    again `LATE` seconds after the last.  `spair` rows carry the camera, the chassis matrix and the new decal."""
    dist = float(os.environ.get("DECAL_DIST", "0.9"))
    fov = float(os.environ.get("POSE_FOV", "0.7"))
    hits = [r for r in rows if r.get("ev") == "hit" and r.get("flag")]
    import collections
    target = collections.Counter(r["ecx"] for r in hits).most_common(1)[0][0]
    first = next(i for i, r in enumerate(rows) if r.get("ev") == "hit" and r.get("flag") and r["ecx"] == target)
    owner = next(x["addr"] for x in rows[first:] if x.get("ev") == "inv")
    decs = [r for r in rows[first:] if r.get("ev") == "decal"]
    side = [r for r in decs if abs(r["u"][1]) < 0.5] or decs
    base_decal = side[len(side) // 2]
    m = sc.exports_sync.m12at(owner)
    print("target part", target, "chassis matrix at", owner, "decal basis alpha", base_decal["alpha"], "u", base_decal["u"], flush=True)
    for i, (dmg, off) in enumerate(zip(damages, offsets)):
        m = sc.exports_sync.m12at(owner)
        ax = [m[0:3], m[3:6], m[6:9]]
        world = lambda v: [sum(v[j] * ax[j][k] for j in range(3)) for k in range(3)]
        c_loc = [base_decal["c"][k] + off * base_decal["v"][k] for k in range(3)]
        c = [a + b for a, b in zip(world(c_loc), m[9:12])]
        nw = world(base_decal["u"])
        ln = math.sqrt(sum(x * x for x in nw)) or 1.0
        nw = [x / ln for x in nw]
        eye = [c[k] + nw[k] * dist for k in range(3)]
        eye[1] += 0.05
        cam = cd.py(EYE_CODE % (*eye, *c, fov)).strip()
        time.sleep(0.4)
        pa = f"{base}-s{i}a.png"
        ra2_drive.shot(DISPLAY, pa)
        n_before = len([r for r in rows if r.get("ev") == "decal"])
        ticket = sc.exports_sync.synth(target, dmg, c, [-x for x in nw])
        ok = None
        for _ in range(100):
            ok = sc.exports_sync.synthdone(ticket)
            if ok is not None:
                break
            time.sleep(0.05)
        time.sleep(0.5)
        new = [r for r in rows if r.get("ev") == "decal"][n_before:]
        mats = {owner: sc.exports_sync.m12at(owner)}
        pb = f"{base}-s{i}b.png"
        ra2_drive.shot(DISPLAY, pb)
        rows.append({"ev": "spair", "i": i, "damage": dmg, "ok": ok, "point": c, "normal": nw, "cam": cam, "fov": fov,
                     "owner": owner, "chassis": mats, "a": os.path.basename(pa), "b": os.path.basename(pb),
                     "decal": new[-1] if new else None})
        print("synth", i, dmg, ok, "new decals", len(new), flush=True)
    late = float(os.environ.get("LATE", "0"))
    if late:
        time.sleep(late)
        pc = f"{base}-late.png"
        ra2_drive.shot(DISPLAY, pc)
        rows.append({"ev": "late", "after": late, "file": os.path.basename(pc)})


def neuter_and_settle():
    print("neuter:", cd.py(cd.NEUTER))
    time.sleep(float(os.environ.get("SETTLE", "7.0")))
    subprocess.run(["python3", os.path.join(HERE, "..", "capture", "xtest.py"), "key", "F3"],
                   env=dict(os.environ, DISPLAY=DISPLAY), check=False)
    time.sleep(1.0)


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
        if os.environ.get("SYNTH"):
            print("neuter:", cd.py(cd.NEUTER))
            time.sleep(1.0)
            pinsc = sess.create_script(open(os.path.join(HERE, "frida", "ra2_pin.js")).read())
            pinsc.load()
            time.sleep(0.5)
            ch = pinsc.exports_sync.chassis()
            bodies = []
            for c0 in ch:
                a0 = int(c0[0], 16)
                near = [b for b in pinsc.exports_sync.scan(hex(a0 - 0x100000), hex(a0 + 0x100000))
                        if b[1] > 0 and math.dist(b[2], c0[2]) < 1.6]
                bodies += [c0] + [b for b in near if b[0] != c0[0]]
            seen, uniq = set(), []
            for b in bodies:
                if b[0] not in seen:
                    seen.add(b[0])
                    uniq.append(b)
            st = pinsc.exports_sync.states([b[0] for b in uniq])
            pinsc.exports_sync.pin([[b[0], s_[0], s_[1]] for b, s_ in zip(uniq, st)])
            print("pinned", len(uniq), "bodies", flush=True)
            pinsc.exports_sync.noImmobility()
            subprocess.run(["python3", os.path.join(HERE, "..", "capture", "xtest.py"), "key", "F3"],
                           env=dict(os.environ, DISPLAY=DISPLAY), check=False)
            time.sleep(1.0)
            synth_decals(base, rows, sc, [float(x) for x in os.environ.get("SYNTH_DAMAGES", "100,60,25").split(",")],
                         [float(x) for x in os.environ.get("SYNTH_OFFSETS", "0,0.35,-0.35").split(",")])
            pinsc.exports_sync.unpin()
            pinsc.unload()
        elif os.environ.get("DECAL_PAIRS"):
            print("neuter:", cd.py(cd.NEUTER))
            time.sleep(float(os.environ.get("SETTLE", "1.5")))
            subprocess.run(["python3", os.path.join(HERE, "..", "capture", "xtest.py"), "key", "F3"],
                           env=dict(os.environ, DISPLAY=DISPLAY), check=False)
            time.sleep(1.0)
            decal_pairs(base, rows, sc, int(os.environ["DECAL_PAIRS"]))
        elif os.environ.get("DECAL_SHOTS"):
            import math as _m  # noqa: F401
            neuter_and_settle()
            n = int(os.environ["DECAL_SHOTS"])
            decal_shots(base, rows, sc, n, "dnat")
            decal_shots(base, rows, sc, n, "dfull", forced=1.0)
        elif os.environ.get("ALPHA_FORCE"):
            # the last 25 decals of the chassis are the live ones: show them at full strength
            objs = [r["obj"] for r in rows if r.get("ev") == "decal"][-25:]
            print("alpha forced on", sc.exports_sync.setalpha(objs, float(os.environ["ALPHA_FORCE"])), "decals")
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
