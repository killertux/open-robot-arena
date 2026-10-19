#!/usr/bin/env python3
"""Fit the capture-timing parameters of the arena stills (fidelity.py ARENA_SKIP / ARENA_INTRO).

capture_arenas.py shoots the intro and clicks `Skip Intro` a fixed WALL time after Start Game;
the intro clock at that moment depends on how long the arena took to load, so it is unknown.
This scans intro-clock moments on a grid (default: the 1/60 s frame step) and prints the MAE of
our frame against the reference at each, plus the best one:

    fit_capture_timing.py compressor intro 2.0:2.3        # arena-<id>-intro vs intro@T
    fit_capture_timing.py electric skip 7.3:7.6           # arena-<id>-countdown after skip@T
    fit_capture_timing.py box intro 6.3:6.7 --ref match-countdown__1024x768__000.png

Only the camera pose and timing are fitted; nothing about the rendering.  Results feed the
FITTED tables in fidelity.py (documented as capture timing, not game behaviour).
"""
import argparse
import concurrent.futures
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageChops, ImageStat

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import fidelity  # noqa: E402


def mae(a, b):
    return sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3.0


def frame(arena, mode, t, out):
    shots = "intro@%g" % t if mode == "intro" else "skip@%g,countdown@2" % t
    bots = ",".join(os.path.join(fidelity.GAME, "AI", "Team0", b) for b in ("Bot0.bot", "Bot1.bot"))
    cmd = [fidelity.APP, "--game", fidelity.GAME, "--match", "--seconds", "5", "--out", out,
           "--size", "%dx%d" % fidelity.SIZE, "--bots", bots,
           "--arena", os.path.join(fidelity.GAME, "Arenas", fidelity.ARENA_SCRIPTS[arena]),
           "--camera-shots", shots]
    earlier = fidelity.earlier_arenas(arena)
    if earlier:
        cmd += ["--earlier-arenas", ",".join(earlier)]
    subprocess.run(cmd, capture_output=True, env=fidelity.env(), timeout=600)
    png = os.path.join(out, "cam-%s.png" % ("intro" if mode == "intro" else "countdown"))
    return Image.open(png).convert("RGB") if os.path.exists(png) else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("arena", choices=sorted(fidelity.ARENA_SCRIPTS))
    ap.add_argument("mode", choices=["intro", "skip"])
    ap.add_argument("range", help="LO:HI intro seconds")
    ap.add_argument("--step", type=float, default=1.0 / 60.0)
    ap.add_argument("--ref", help="reference file in local/oracle/original (default: the arena's)")
    ap.add_argument("--jobs", type=int, default=3)
    a = ap.parse_args()
    lo, hi = (float(x) for x in a.range.split(":"))
    shot = "intro" if a.mode == "intro" else "countdown"
    ref_name = a.ref or "arena-%s-%s__1024x768__000.png" % (a.arena, shot)
    ref = Image.open(os.path.join(fidelity.ORIG, ref_name)).convert("RGB")
    n = int(round((hi - lo) / a.step))
    times = [round(lo + i * a.step, 4) for i in range(n + 1)]
    tmp = tempfile.mkdtemp(prefix="ra2-fit-")

    def one(t):
        d = os.path.join(tmp, "%.4f" % t)
        os.makedirs(d, exist_ok=True)
        im = frame(a.arena, a.mode, t, d)
        return t, (mae(im, ref) if im else float("nan"))

    with concurrent.futures.ThreadPoolExecutor(a.jobs) as pool:
        rows = list(pool.map(one, times))
    for t, m in rows:
        print("%8.4f  %7.2f" % (t, m))
    good = [r for r in rows if r[1] == r[1]]
    if good:
        t, m = min(good, key=lambda r: r[1])
        print("best %s %s@%g  MAE %.2f  (ref %s)" % (a.arena, a.mode, t, m, ref_name))


if __name__ == "__main__":
    main()
