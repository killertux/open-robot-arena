#!/usr/bin/env python3
"""State-driven arena captures from the ORIGINAL (docs/34 section 9).

Unlike capture_arenas.py (fixed wall-time waits) every shot here waits on a game state seen by the
frida agent tools/oracle/frida/ra2_matchstate.js:

    intro_state      0x4758c0  the intro flight is running (Skip Intro shown)
    countdown_start  0x4759c0  the 3-2-1 countdown starts (Skip click or the intro's own end)
    fight_start      0x416360  the fight begins
    update           0x477480  per-frame match update; state carries the intro/dt clock

Per arena row (from the Exhibition screen with both robot slots filled, DEATHMATCH, hazards on):

    arena-<n>-intro2      when the intro clock (summed frame dt since the intro state, = 3.0 - the camera label's timer) reaches --intro-at seconds
    arena-<n>-countdown2  the intro runs to its end (no skip); shot when the countdown reads --cd-at
    arena-<n>-f4live      (--live-f4) F4 pressed 2 s into the fight, while the match is still on

and, for the Combat Arena, match-countdown2 (the same countdown frame: COMPETITORS READY panel).
The state of the clocks at the shot is written next to the manifest row.  Texture-manager and file
open events are written to the --events jsonl for the session-cache questions.

    capture_arena_states.py [--rows 1,5] [--names ...] [--intro-at 2.3] [--cd-at 2.0]
"""
import argparse
import json
import os
import queue
import subprocess
import sys
import time

import frida

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import ra2_drive  # noqa: E402

PY = os.environ.get("RA2_FRIDA_PYTHON", os.path.expanduser("~/tools/frida-venv/bin/python3"))
NAMES = ["bridge", "box", "compressor", "electric", "halfcircle", "metalskull", "parkinglot", "octagon"]
DISPLAY = os.environ.get("RA2_CAPTURE_DISPLAY", ":5")
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


class Agent:
    def __init__(self, events_path):
        dev = frida.get_device_manager().add_remote_device(HOST)
        pid = [p.pid for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0]
        self.session = dev.attach(pid)
        self.q = queue.Queue()
        self.log = open(events_path, "a")
        with open(os.path.join(HERE, "frida", "ra2_matchstate.js")) as f:
            self.script = self.session.create_script(f.read())
        self.script.on("message", self.on_message)
        self.script.load()

    def on_message(self, msg, data):
        if msg.get("type") != "send":
            print("agent error:", msg, file=sys.stderr)
            return
        p = msg["payload"]
        p["wall"] = time.time()
        self.log.write(json.dumps(p) + "\n")
        self.log.flush()
        self.q.put(p)

    def state(self):
        return self.script.exports_sync.state()

    def drain(self):
        while not self.q.empty():
            self.q.get_nowait()

    def wait(self, name, timeout=90):
        end = time.time() + timeout
        while time.time() < end:
            try:
                p = self.q.get(timeout=0.5)
            except queue.Empty:
                continue
            if p.get("ev") == name:
                return p
        raise SystemExit(f"timeout waiting for {name}")

    def wait_clock(self, key, pred, timeout=60):
        end = time.time() + timeout
        while time.time() < end:
            s = self.state()
            if s and pred(s[key]):
                return s
            time.sleep(0.01)
        raise SystemExit(f"timeout waiting for clock {key}")


def raw_shot(path):
    """Grab the root window right now (xwd, ~20 ms); convert later with finish_shots()."""
    subprocess.run(["xwd", "-root", "-silent", "-out", path + ".xwd"],
                   env=dict(os.environ, DISPLAY=DISPLAY), check=True)


def finish_shots(paths):
    for p in paths:
        subprocess.run(["python3", ra2_drive.XWD2PNG, p + ".xwd", p, "--quiet"],
                       check=True, stdout=subprocess.DEVNULL)
        os.remove(p + ".xwd")


def drive(steps):
    p = subprocess.run([PY, os.path.join(HERE, "ra2_drive.py")] + steps,
                       capture_output=True, text=True, timeout=240)
    if p.returncode:
        sys.exit(f"ra2_drive failed: {p.stderr[-500:]}")


def python(code):
    subprocess.run([PY, os.path.join(HERE, "ra2_python.py"), "-c", code],
                   capture_output=True, text=True, timeout=120)


def add(fid, path, state):
    subprocess.run(["python3", os.path.join(HERE, "add_fixture.py"), fid, path, state],
                   check=True, capture_output=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rows", default="0-7")
    ap.add_argument("--intro-at", type=float, default=2.3)
    ap.add_argument("--cd-at", type=float, default=2.0)
    ap.add_argument("--live-f4", default="metalskull")
    ap.add_argument("--events", default="/tmp/ra2-arena-states-events.jsonl")
    ap.add_argument("--no-add", action="store_true", help="only print, do not add fixtures")
    ap.add_argument("--tmp", default="/tmp/ra2-arena-states")
    a = ap.parse_args()
    os.makedirs(a.tmp, exist_ok=True)
    if "-" in a.rows:
        lo, hi = (int(x) for x in a.rows.split("-"))
        rows = list(range(lo, hi + 1))
    else:
        rows = [int(x) for x in a.rows.split(",")]
    ag = Agent(a.events)
    results = {}
    for row in rows:
        name = NAMES[row]
        y = 170 + 18 * row
        base = f"EXHIBITION > Arena row {row} ({name}), DEATHMATCH, Red Zone Scout vs ALARM AI, hazards on"
        drive(["click", "176", "177", "wait", "1.5", "click", "322", str(y), "wait", "1.5", "hide", "wait", "1"])
        ag.drain()
        drive(["click", "936", "647", "hide"])
        ev = ag.wait("intro_state", 120)
        # the intro clock is the match shell's summed dt (this+0x1c0)
        s_intro = ag.wait_clock("ic", lambda v: v >= a.intro_at)
        ip = os.path.join(a.tmp, f"{name}-intro2.png")
        raw_shot(ip)
        s_intro2 = ag.state()
        # let the intro finish by itself
        cd = ag.wait("countdown_start", 120)
        s_cd = ag.wait_clock("cd", lambda v: v <= a.cd_at)
        cp = os.path.join(a.tmp, f"{name}-countdown2.png")
        raw_shot(cp)
        s_cd2 = ag.state()
        fight = ag.wait("fight_start", 30)
        info = dict(intro=[s_intro, s_intro2], countdown=[s_cd, s_cd2], countdown_start=cd["s"],
                    fight_start=fight["s"], intro_state=ev["s"])
        results[name] = info
        shots = {"intro2": ip, "countdown2": cp}
        if name in a.live_f4.split(","):
            time.sleep(2)
            drive(["key", "F4", "wait", "1.0"])
            fp = os.path.join(a.tmp, f"{name}-f4live.png")
            raw_shot(fp)
            info["f4live"] = ag.state()
            shots["f4live"] = fp
        finish_shots(shots.values())
        print(json.dumps({name: info}), flush=True)
        if not a.no_add:
            def st(k):
                return json.dumps(info.get(k) if k in info else None)
            add(f"arena-{name}-intro2", ip, base + f"; intro flight at intro clock (summed dt since 0x4758c0) {s_intro['ic']:.3f}..{s_intro2['ic']:.3f} s "
                f"(state hook 0x4758c0/0x477480, tools/oracle/capture_arena_states.py)")
            add(f"arena-{name}-countdown2", cp, base + f"; intro ran to its end (no skip); countdown reads {s_cd['cd']:.3f}..{s_cd2['cd']:.3f} s "
                f"(of 4.0), countdown_start hook 0x4759c0")
            if name == "box":
                add("match-countdown2", cp, base + "; real match countdown (COMPETITORS READY panel), same frame as arena-box-countdown2")
            if "f4live" in shots:
                add(f"arena-{name}-f4live", shots["f4live"], base + "; F4 pressed 2 s into the fight, match still on (fight_start hook 0x416360)")
        # end the match and go back
        python("import plus; plus.eliminatePlayer(1)")
        time.sleep(6)
        drive(["click", "717", "575", "hide", "wait", "3"])
    json.dump(results, open(os.path.join(a.tmp, "states.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
