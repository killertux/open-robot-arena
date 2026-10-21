#!/usr/bin/env python3
"""Turn a capture_fx.py run with POSE=1 into the plain-text spec `ra2sim --example decalshot` reads.

  fx_decal_spec.py fx-<name>__live__<seq>.jsonl BOT0 BOT1 > spec.txt

Lines:
  bot <player> <path relative to the game dir>
  mark <player> <file> <mode> <w> <h> <alpha> <c xyz> <normal xyz> <axis xyz>   (oldest first, at most the last 25)
  shot <name> <player> <eye xyz> <pitch> <yaw> <fov> <chassis 0: 12 floats> <chassis 1: 12 floats>
      the scripted camera and both chassis objects' matrices (rows = axes, then the translation) at that shot
A decal belongs to the chassis whose matrix address its hit chain (`inv` row) read.
"""
import json
import math
import sys


def main():
    rows = [json.loads(line) for line in open(sys.argv[1])]
    bots = sys.argv[2:4]
    poses = [r for r in rows if r["ev"] == "pose"]
    if not poses:
        sys.exit("no pose rows (POSE=1)")
    # chassis addr -> player: the matrix translation nearest the player's location
    first = poses[0]
    owner = {}
    for addr, m in first["chassis"].items():
        best = None
        for p in poses:
            loc = eval(p["pose"])[0]
            d = math.dist(loc, m[9:12])
            if best is None or d < best[0]:
                best = (d, p["player"])
        owner[addr] = best[1]
    for p, b in enumerate(bots):
        print(f"bot {p} {b}")
    last = None
    marks = {0: [], 1: []}
    for r in rows:
        if r["ev"] == "hit":
            hit = r
        elif r["ev"] == "inv":
            last = owner.get(r["addr"])
        elif r["ev"] == "decal" and last is not None:
            f = lambda v: " ".join("%.6f" % x for x in v)
            mode = hit.get("mode", 0) if hit.get("flag") else 0
            marks[last].append(f"mark {last} {r['file'].split(chr(92))[-1]} {mode} {r['w']:.4f} {r['h']:.4f} "
                               f"{r['alpha']:.6f} {f(r['c'])} {f(r['u'])} {f(r['v'])}")
    for p in (0, 1):
        for line in marks[p][-25:]:
            print(line)
    for k, p in enumerate(poses):
        cam = eval(p["cam"])
        by_player = {owner[a]: m for a, m in p["chassis"].items()}
        mats = " ".join("%.6f" % x for pl in (0, 1) for x in by_player[pl])
        print("shot pose%02d %d %.6f %.6f %.6f %.6f %.6f %.4f %s" % (k, p["player"], *cam[0], cam[1], cam[2], p["fov"], mats))


if __name__ == "__main__":
    main()
