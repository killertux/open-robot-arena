#!/usr/bin/env python3
"""Record the ORIGINAL's audio and its sound-call log for N seconds.

    capture_audio.py NAME SECONDS            # then drive the game meanwhile

Needs `tools/capture/original_session.sh start`.  It runs, side by side:
  * parec on the "Robot Arena 2.exe" sink input (the game's own stream, 48 kHz
    stereo s16le) -> <OUT>/audio-NAME__live__NNN.wav
  * frida/ra2_sounds.js (every SoundWizard / UI sound call, with Date.now()
    stamps) -> <OUT>/audio-NAME__live__NNN.jsonl

The first line of the .jsonl is {"ev":"meta","parec_epoch_ms":...,...}: the host
wall-clock just before parec started, the same clock as the agent's `ms`
fields, so a log row at epoch E sits at (E - parec_epoch_ms)/1000 s in the WAV
(+ a ~0.05 s parec start latency that `audio_align.py` / the reports measure).
OUT defaults to local/oracle/original; a manifest row is appended
(manifest.tsv columns: see docs/12 section 6).
"""
import json
import os
import subprocess
import sys
import threading
import time

import frida

HERE = os.path.dirname(os.path.abspath(__file__))
LOCAL = os.path.join(os.path.dirname(os.path.dirname(HERE)), "local")
ORIG = os.path.join(LOCAL, "oracle", "original")
AGENT = os.environ.get("RA2_AUDIO_AGENT") or os.path.join(HERE, "frida", "ra2_sounds.js")
HOST = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")


def sink_input():
    out = subprocess.run(["pactl", "list", "sink-inputs"], capture_output=True, text=True).stdout
    cur = None
    for line in out.splitlines():
        if line.startswith("Sink Input #"):
            cur = line.split("#")[1].strip()
        if 'application.name = "Robot Arena 2.exe"' in line:
            return cur
    sys.exit("capture_audio: no 'Robot Arena 2.exe' sink input (is the game running?)")


def main():
    name, secs = sys.argv[1], float(sys.argv[2])
    out_dir = os.environ.get("OUT", ORIG)
    n = 0
    while os.path.exists(os.path.join(out_dir, f"audio-{name}__live__{n:03d}.jsonl")):
        n += 1
    base = os.path.join(out_dir, f"audio-{name}__live__{n:03d}")
    dev = frida.get_device_manager().add_remote_device(HOST)
    pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
    sess = dev.attach(pid)
    rows = []
    # rows are appended to the .jsonl as they arrive, so a killed run keeps them
    out = open(base + ".jsonl", "w")

    def on_message(m, d):
        if m.get("type") == "send" and m["payload"].get("ev") == "batch":
            for r in m["payload"]["rows"]:
                rows.append(r)
                out.write(json.dumps(r, separators=(",", ":")) + "\n")
            out.flush()
        elif m.get("type") != "send":
            print("agent", m, file=sys.stderr)

    sc = sess.create_script(open(AGENT).read())
    sc.on("message", on_message)
    idx = sink_input()
    t_parec = int(time.time() * 1000)
    out.write(json.dumps({"ev": "meta", "parec_epoch_ms": t_parec, "seconds": secs, "name": name}) + "\n")
    out.flush()
    p = subprocess.Popen(["parec", f"--monitor-stream={idx}", "--file-format=wav", "--format=s16le",
                          "--rate=48000", "--channels=2", base + ".wav"], stderr=subprocess.DEVNULL)
    sc.load()
    print("capturing", base, flush=True)
    t0 = time.time()
    try:
        while time.time() - t0 < secs and not os.path.exists(base + ".stop"):
            time.sleep(0.2)
    except KeyboardInterrupt:
        pass
    p.terminate()
    p.wait()
    sc.exports_sync.flush()
    time.sleep(0.4)
    sc.unload()
    sess.detach()
    out.close()
    print("wrote", base, len(rows), "rows")


if __name__ == "__main__":
    main()
