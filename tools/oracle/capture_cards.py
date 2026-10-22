#!/usr/bin/env python3
"""The ORIGINAL's out cards and dimmed status group (docs/32 section 7), and the green damage number.

Needs `tools/capture/original_session.sh start` and the game at the main menu.  Human C.R.U.S.H. vs
ALARM in the Combat Arena (hazards off, both AIs stopped); with frida/ra2_cards.js (and ra2_numbers.js
for STAGE=green) loaded, one stage per match:

  STAGE=a+b runs the stages one after the other in the same match (a fight that ends ends them all).

  elim      `plus.eliminatePlayer(1)`: state 3, the `eliminated.tga` card
  destroy   `plus.damage(1, 0, 20000, point)` x4: the chassis criticals, state 1, `controlboard_destroyed.tga`
  green     the human driven into the stopped ALARM: the green attacker numbers (docs/37 section 7)

  NAME=hudcards-elim STAGE=elim OUT=<dir> tools/oracle/capture_cards.py

Writes OUT/<NAME>__live__<seq>.jsonl (agent rows + shot rows) and ...-<k>.png (every EVERY-th burst
frame, ~15 per second).  Nothing in the game directory is touched.
"""
import math
import os
import subprocess
import sys
import time

import capture_numbers as cn
import capture_drive as cd
import frida

HERE = cn.HERE


def run_stage(stage, g, py, poll, secs):
    if True:
        if stage == "elim":
            g.start()
            time.sleep(1.0)
            with cn.lock:
                cn.rows.append({"ev": "action", "what": "eliminatePlayer(1)", "host_t": time.time()})
            print(py.run("import plus; plus.eliminatePlayer(1)"), flush=True)
            time.sleep(secs)
        elif stage == "destroy":
            g.start()
            time.sleep(1.0)
            poll("start")
            for i in range(int(os.environ.get("BLOWS", "4"))):
                with cn.lock:
                    cn.rows.append({"ev": "action", "what": f"damage blow {i}", "host_t": time.time()})
                print(py.run("import plus\nl = plus.getLocation(1)\nplus.damage(1, 0, 20000.0, (l[0], l[1] + 0.3, l[2]))\nout.write(repr(l))"), flush=True)
                time.sleep(float(os.environ.get("GAP", "0.6")))
                poll(f"after blow {i}")
            for j in range(int(secs / 1.5)):
                time.sleep(1.5)
                poll(f"wait {j}")
        elif stage == "green":
            hold = float(os.environ.get("HOLD", "14"))
            XT = cn.XTEST

            def key(kk, d):
                subprocess.run([sys.executable, XT, "--display", cn.DISPLAY, "key", kk, d], stdout=subprocess.DEVNULL)

            if os.environ.get("ALARM_GO"):
                py.run("import AI\nAI.running_ai[0].Throttle(%s)\nAI.running_ai[0].Turn(0)" % os.environ["ALARM_GO"])
            key("w", "down")
            g.start()
            t0 = time.time()
            cur = None
            sign = float(os.environ.get("TURN_SIGN", "1"))
            while time.time() - t0 < hold:
                s = py.run("import plus\nout.write(repr((plus.getLocation(0), plus.getHeading(0), plus.getLocation(1))))")
                try:
                    a0, h, a1 = eval(s)
                except Exception:
                    continue
                err = (math.atan2(a1[0] - a0[0], a1[2] - a0[2]) - h + math.pi) % (2 * math.pi) - math.pi
                with cn.lock:
                    cn.rows.append({"ev": "aim", "err": err, "dist": math.hypot(a1[0] - a0[0], a1[2] - a0[2]), "pose": [a0, h, a1], "host_t": time.time()})
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


def main():
    name = os.environ.get("NAME", "hudcards")
    stage = os.environ.get("STAGE", "elim")
    out_dir = os.environ.get("OUT", cd.ORIG)
    secs = float(os.environ.get("SECS", "9"))
    every = int(os.environ.get("EVERY", "2"))
    dev = frida.get_device_manager().add_remote_device(cn.HOST)
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
        with cn.lock:
            cn.rows.append(p)

    scripts = []
    if not os.environ.get("NOHOOK"):
        scripts.append(sess.create_script(open(os.path.join(HERE, "frida", os.environ.get("CARDS_JS", "ra2_cards.js"))).read()))
    if "green" in stage:
        scripts.append(sess.create_script(open(os.path.join(HERE, "frida", "ra2_numbers.js")).read()))
    for sc in scripts:
        sc.on("message", on_msg)
        sc.load()
    if os.environ.get("MATCH") != "running":
        cd.drive(["-f", os.path.join(HERE, "routes", "exhibition_box_crush_alarm.steps")])
        cd.drive(["click", "176", "177", "wait", "1.5", "click", "322", "188", "wait", "1", "click", "295", "487", "wait", "0.5", "hide"])
        cd.drive(["click", "936", "647", "hide", "wait", "14", "click", "512", "735", "hide", "wait", "1"])
    py = cn.Py()
    print("neuter:", py.run(cd.NEUTER), flush=True)
    k = 0

    def poll(why):
        if not scripts or os.environ.get("NOPOLL"):
            return
        try:
            data = scripts[0].exports_sync.poll()
        except Exception as e:
            data = str(e)
        with cn.lock:
            cn.rows.append({"ev": "poll", "why": why, "meters": data, "host_t": time.time()})

    try:
        for st in stage.split("+"):
            g = cn.Grab()
            if os.environ.get("NOGRAB"):
                g.start = lambda: None
                g.stop = lambda: None
                g.save = lambda base, k, why, rows, every=1: k
            run_stage(st, g, py, poll, secs)
            k = g.save(base, k, st, cn.rows, every=every)
    finally:
        cn.flush(base)
        for sc in scripts:
            try:
                sc.unload()
            except Exception:
                pass
        sess.detach()
        counts = {}
        for r in cn.rows:
            counts[r["ev"]] = counts.get(r["ev"], 0) + 1
        print("wrote", base, counts)
        py.session.detach()


if __name__ == "__main__":
    main()
