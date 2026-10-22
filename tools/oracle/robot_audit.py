#!/usr/bin/env python3
"""Pose-matched robot audit (docs/34 section 11): live shots against ours, per shot and per robot.

    robot_audit.py <live_prefix> <ours_dir> <sbs_dir> [--rows out.tsv]

`<live_prefix>` is a capture_shape.py output base (`.../audit_x__live__000`, with its `.jsonl` and `-NN.png`
files), `<ours_dir>` holds the `shape_<k>.png` of `shapeshot` run with `RA2_SHAPE_SET=audit` (no
poses: both sides put the camera relative to their own robot at its spawn point).  For every shot it writes `<sbs_dir>/<stem>_<k>_live-left-ours-right.png`
and prints: the whole-frame MAE (0..255, HUD clock and the two HUD cards masked), the fraction of pixels more than
24 apart, and the 16x16 block with the largest mean difference.  Standard library only (compare.py's PNG code).
"""
import ast
import json
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import compare as cmp  # noqa: E402

MASKS = [(440, 700, 140, 50), (0, 0, 130, 90), (900, 0, 124, 90)]  # clock, the two HUD cards


def masked(x, y):
    return any(rx <= x < rx + rw and ry <= y < ry + rh for rx, ry, rw, rh in MASKS)


CENTRE = (192, 176, 832, 600)  # the robot's window (x0, y0, x1, y1): the cameras look at it


def centre_mae(a, b, w):
    tot = n = 0
    for y in range(CENTRE[1], CENTRE[3]):
        for x in range(CENTRE[0], CENTRE[2]):
            if masked(x, y):
                continue
            i = (y * w + x) * 3
            tot += abs(a[i] - b[i]) + abs(a[i + 1] - b[i + 1]) + abs(a[i + 2] - b[i + 2])
            n += 3
    return tot / n


def measure(a, b, w, h):
    tot = n = big = 0
    blocks = {}
    for y in range(h):
        row = y * w * 3
        for x in range(w):
            if masked(x, y):
                continue
            i = row + x * 3
            d = max(abs(a[i] - b[i]), abs(a[i + 1] - b[i + 1]), abs(a[i + 2] - b[i + 2]))
            tot += abs(a[i] - b[i]) + abs(a[i + 1] - b[i + 1]) + abs(a[i + 2] - b[i + 2])
            n += 3
            if d > 24:
                big += 1
            k = (x // 16, y // 16)
            blocks[k] = blocks.get(k, 0) + d
    worst = max(blocks.items(), key=lambda kv: kv[1])
    return tot / n, big / (n / 3), (worst[0][0] * 16, worst[0][1] * 16, worst[1] / 256.0)


def sbs(a, b, w, h, path):
    out = bytearray()
    for y in range(h):
        out += a[y * w * 3:(y + 1) * w * 3] + b[y * w * 3:(y + 1) * w * 3]
    cmp.write_png(path, w * 2, h, bytes(out))


def ours_poses(ours):
    """`shot k: robot p at Vec3(x, y, z) heading Vec3(fx, 0, fz)` lines of shapeshot's log -> {k: (x, z, yaw)}."""
    out = {}
    try:
        for line in open(os.path.join(ours, "log.txt")):
            m = re.match(r"shot (\d+): robot \d+ at Vec3\(([-\d.]+), ([-\d.]+), ([-\d.]+)\) heading Vec3\(([-\d.]+), [-\d.]+, ([-\d.]+)\)", line)
            if m:
                out[int(m[1])] = (float(m[2]), float(m[4]), math.atan2(float(m[5]), float(m[6])))
    except OSError:
        pass
    return out


def half(rgb, w, h):
    out = bytearray()
    for y in range(0, h - 1, 2):
        r0, r1 = y * w * 3, (y + 1) * w * 3
        for x in range(0, w - 1, 2):
            for c in range(3):
                i = x * 3 + c
                out.append((rgb[r0 + i] + rgb[r0 + i + 3] + rgb[r1 + i] + rgb[r1 + i + 3]) // 4)
    return bytes(out), w // 2, h // 2


def montage(pairs, path):
    """One image per robot: its shots, live on the left and ours on the right, half size, stacked."""
    rows = []
    for a, b, w, h in pairs:
        ha, hw, hh = half(a, w, h)
        hb, _, _ = half(b, w, h)
        for y in range(hh):
            rows.append(ha[y * hw * 3:(y + 1) * hw * 3] + hb[y * hw * 3:(y + 1) * hw * 3])
    cmp.write_png(path, hw * 2, len(rows), b"".join(rows))


def main():
    live, ours, sdir = sys.argv[1:4]
    rows = None
    if "--rows" in sys.argv:
        rows = open(sys.argv[sys.argv.index("--rows") + 1], "a")
    os.makedirs(sdir, exist_ok=True)
    stem = os.path.basename(live)
    shots = [json.loads(l) for l in open(live + ".jsonl")]
    oposes = ours_poses(ours)
    montages = {}
    for k, s in enumerate(shots):
        lp = f"{live}-{k:02d}.png"
        op = os.path.join(ours, f"shape_{k}.png")
        if not (os.path.exists(lp) and os.path.exists(op)):
            continue
        w, h, a = cmp.read_png(lp)
        w2, h2, b = cmp.read_png(op)
        if (w, h) != (w2, h2):
            print("size mismatch", lp, op)
            continue
        mae, frac, worst = measure(a, b, w, h)
        cmae = centre_mae(a, b, w)
        for p in range(2):
            if k // 4 == p:
                montages.setdefault(p, []).append((a, b, w, h))
        sbs(a, b, w, h, os.path.join(sdir, f"{stem}_{k}_live-left-ours-right.png"))
        a, h = ast.literal_eval(s["pose"])[:2]
        dyaw = dpos = float("nan")
        if k in oposes:
            dyaw = (oposes[k][2] - h + math.pi) % (2 * math.pi) - math.pi
            dpos = math.hypot(oposes[k][0] - a[0], oposes[k][1] - a[2])
        line = f"{stem}\t{k}\t{s['player']}\t{s['bot']}\t{mae:.3f}\t{cmae:.3f}\t{frac * 100:.2f}\t{worst[0]},{worst[1]},{worst[2]:.1f}\t{dyaw:.3f}\t{dpos:.3f}"
        print(line)
        if rows:
            rows.write(line + "\n")
    for p, items in montages.items():
        montage(items, os.path.join(sdir, f"{stem}_robot{p}_live-left-ours-right.png"))


if __name__ == "__main__":
    main()