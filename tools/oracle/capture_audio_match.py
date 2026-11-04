#!/usr/bin/env python3
"""Audio capture of one whole AI-vs-AI exhibition match in the ORIGINAL.

Needs `tools/capture/original_session.sh start` with the game on the main menu
(or anywhere `ra2_drive.py click 527 746` reaches it).  Sets up Combat Arena,
Scout vs ALARM (routes/exhibition_box_scout_alarm.steps), starts capture_audio.py,
clicks Start Game WITHOUT skipping the intro, lets the intro, the countdown and
FIGHT_SECONDS of the fight play (both shipped AIs fight), then ends the match
(plus.eliminatePlayer) and records the result screen for RESULT_SECONDS.
Wall-clock markers go to <base>.marks.jsonl (epoch ms, same clock as the sound log).
Environment: NAME (default match-box-scout-alarm), FIGHT_SECONDS (default 60),
RESULT_SECONDS (default 25), ARENA_ROW (exhibition arena list row, default the route's Combat Arena), SKIP_INTRO=1 (click Skip Intro 14 s after Start Game), INTRO_WAIT (seconds to wait after Start Game before
the fight clock is checked, default 75).
"""
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
PY = os.environ.get("RA2_FRIDA_PYTHON", os.path.expanduser("~/tools/frida-venv/bin/python3"))
ORIG = os.path.join(os.path.dirname(os.path.dirname(HERE)), "local", "oracle", "original")


def run(cmd, timeout=45):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout).stdout
    except subprocess.TimeoutExpired:
        print("TIMEOUT", cmd[-3:], flush=True)
        return ""


def drive(*steps):
    return run([PY, os.path.join(HERE, "ra2_drive.py"), *steps])


def py(code):
    return run([PY, os.path.join(HERE, "ra2_python.py"), "-c", code])


def main():
    name = os.environ.get("NAME", "match-box-scout-alarm")
    fight = float(os.environ.get("FIGHT_SECONDS", "60"))
    result = float(os.environ.get("RESULT_SECONDS", "25"))
    out_dir = os.environ.get("OUT", ORIG)
    total = 12 + float(os.environ.get("INTRO_WAIT", "75")) + fight + result + 30
    n = 0
    while os.path.exists(os.path.join(out_dir, f"audio-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"audio-{name}__live__{n:03d}")
    marks = []

    def mark(label, **kw):
        marks.append({"t": int(time.time() * 1000), "label": label, **kw})
        print(label, kw, flush=True)

    if os.environ.get("AT_MENU") != "1":  # AT_MENU=1: the game is on the main menu (that click opens Credits there)
        drive("click", "527", "746", "wait", "2")
    drive("-f", os.path.join(HERE, "routes", "exhibition_box_scout_alarm.steps"))
    if os.environ.get("ARENA_ROW"):  # another arena: its row in the exhibition list (box 1, compressor 2, octagon 7)
        drive("click", "176", "177", "wait", "1.5", "click", "322", str(170 + 18 * int(os.environ["ARENA_ROW"])), "wait", "1.5", "hide", "wait", "1.5")
    cap = subprocess.Popen([PY, os.path.join(HERE, "capture_audio.py"), name, str(total)], env=dict(os.environ, OUT=out_dir))
    time.sleep(5)
    mark("click_start_game")
    drive("click", "936", "647", "hide")
    t0 = time.time()
    if os.environ.get("SKIP_INTRO") == "1":
        time.sleep(14)
        mark("skip_intro")
        drive("click", "512", "735", "hide")
    clock = ""
    # poll the fight clock (plus.getTimeElapsed) until the match is running
    while time.time() - t0 < float(os.environ.get("INTRO_WAIT", "75")) + 60:
        time.sleep(4)
        clock = py("import plus; out.write(repr(plus.getTimeElapsed()))").strip()
        mark("poll", elapsed=clock)
        try:
            if float(clock) > 0.5:
                break
        except ValueError:
            pass
    mark("fight_started", elapsed=clock)
    t1 = time.time()
    while time.time() - t1 < fight:
        time.sleep(5)
        mark("poll", elapsed=py("import plus; out.write(repr(plus.getTimeElapsed()))").strip())
    mark("eliminate", out=py("import plus; plus.eliminatePlayer(1)").strip())
    time.sleep(result)
    mark("result_click_continue")
    drive("click", "717", "575", "hide", "wait", "3")
    mark("end")
    open(base + ".stop", "w").close()
    cap.wait()
    os.remove(base + ".stop")
    with open(base + ".marks.jsonl", "w") as f:
        for m in marks:
            f.write(json.dumps(m) + "\n")
    print("wrote", base)


if __name__ == "__main__":
    main()
