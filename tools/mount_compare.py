#!/usr/bin/env python3
"""Score a robot's placement against the game's own embedded snapshot.

Every bot file carries a 128x128 preview the game rendered of itself, which makes
it ground truth for how components are assembled. Comparing our render to it
needs two things: the snapshot decoded, and the camera found. The chassis and
wheels are placed the same way by every mount rule, so they can be used to *fit*
the camera; only then is the rest of the silhouette meaningful.

Similarity is bounding-box-normalised IoU, so framing and scale differences
between the game's renderer and ours cancel out and only shape/pose is compared.

Usage:
    tools/mount_compare.py <bot> [--modes stored,frames,frames-flip]
                                 [--camera "ex,ey,ez,tx,ty,tz"]

With `--camera` the search is skipped (about 20x faster); pass the best camera it
printed earlier. Requires Pillow and a built `ra2-render` at
`engine/target/release/ra2-render`.

Context: `docs/06-deferred-fidelity.md` F1.
"""

import argparse
import math
import os
import struct
import subprocess
import sys
import tempfile
import zlib
from collections import deque

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RENDER = os.path.join(ROOT, "engine/target/release/ra2-render")
SIZE = 128
TARGET = (0.0, 0.35, 0.0)
DIST = 3.2


def embedded_snapshot(bot_path, out_png):
    """Decode the 128x128 TGA the .bot carries, as a PNG. Returns False if absent."""
    data = open(bot_path, "rb").read()
    pos = 0

    def line():
        nonlocal pos
        end = data.find(b"\n", pos)
        s = data[pos:end]
        pos = end + 1
        return s

    version = line().decode().strip()
    line()  # name
    line()  # class
    if version != "1.07":
        line()  # default texture index
    if line().strip() != b"true":
        return False
    tga = data[pos : pos + 18 + SIZE * SIZE * 4]
    if len(tga) < 18 + SIZE * SIZE * 4:
        return False
    w = struct.unpack("<H", tga[12:14])[0]
    h = struct.unpack("<H", tga[14:16])[0]
    px = tga[18:]
    out = Image.new("RGB", (w, h))
    out.putdata(
        [
            (px[(y * w + x) * 4 + 2], px[(y * w + x) * 4 + 1], px[(y * w + x) * 4])
            for y in range(h - 1, -1, -1)
            for x in range(w)
        ]
    )
    out.save(out_png)
    return True


def foreground(path, tol=42):
    """Mask of pixels not reachable from the border through similar colours.

    A flood fill rather than a threshold on one pixel: the game's snapshots have a
    background gradient that a fixed threshold mistakes for geometry.
    """
    im = Image.open(path).convert("RGB")
    w, h = im.size
    px = im.load()
    bg = set()
    queue = deque()
    for x in range(w):
        queue.append((x, 0))
        queue.append((x, h - 1))
    for y in range(h):
        queue.append((0, y))
        queue.append((w - 1, y))
    while queue:
        x, y = queue.popleft()
        if (x, y) in bg:
            continue
        bg.add((x, y))
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            nx, ny = x + dx, y + dy
            if 0 <= nx < w and 0 <= ny < h and (nx, ny) not in bg:
                a, b = px[x, y], px[nx, ny]
                if sum(abs(a[k] - b[k]) for k in range(3)) <= tol:
                    queue.append((nx, ny))
    return [[0 if (x, y) in bg else 1 for x in range(w)] for y in range(h)]


def normalised(mask, size=48):
    ys = [y for y, row in enumerate(mask) if any(row)]
    xs = [x for x in range(len(mask[0])) if any(mask[y][x] for y in range(len(mask)))]
    if not ys or not xs:
        return [[0] * size for _ in range(size)]
    y0, y1, x0, x1 = min(ys), max(ys), min(xs), max(xs)
    fh, fw = max(1, y1 - y0 + 1), max(1, x1 - x0 + 1)
    return [
        [
            mask[min(y0 + r * fh // size, len(mask) - 1)][
                min(x0 + c * fw // size, len(mask[0]) - 1)
            ]
            for c in range(size)
        ]
        for r in range(size)
    ]


def iou(a, b):
    inter = union = 0
    for ra, rb in zip(a, b):
        for va, vb in zip(ra, rb):
            if va or vb:
                union += 1
                if va and vb:
                    inter += 1
    return inter / union if union else 0.0


def render(bot, out, camera, mount):
    env = dict(os.environ)
    env["RA2_CAM"] = camera
    if mount:
        env["RA2_MOUNT"] = mount
    else:
        env.pop("RA2_MOUNT", None)
    subprocess.run(
        [RENDER, bot, out, str(SIZE), str(SIZE)],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bot")
    ap.add_argument("--modes", default="stored,frames,frames-flip")
    ap.add_argument("--camera")
    args = ap.parse_args()

    if not os.path.exists(RENDER):
        sys.exit(f"build it first: cargo build --release -p ra2-render ({RENDER})")

    tmp = tempfile.mkdtemp(prefix="ra2mount")
    snap = os.path.join(tmp, "snapshot.png")
    if not embedded_snapshot(args.bot, snap):
        sys.exit("this .bot carries no embedded snapshot")
    target = normalised(foreground(snap))

    camera = args.camera
    if not camera:
        best = []
        for az in range(0, 360, 20):
            for el in (20, 35, 50, 65, 80):
                a, e = math.radians(az), math.radians(el)
                eye = (
                    TARGET[0] + DIST * math.cos(e) * math.sin(a),
                    TARGET[1] + DIST * math.sin(e),
                    TARGET[2] + DIST * math.cos(e) * math.cos(a),
                )
                cam = ",".join(f"{v:.4f}" for v in eye + TARGET)
                out = os.path.join(tmp, "cam.png")
                render(args.bot, out, cam, None)
                best.append((iou(normalised(foreground(out)), target), cam))
        best.sort(reverse=True)
        camera = best[0][1]
        print("best cameras (IoU):")
        for s, cam in best[:5]:
            print(f"  {s:.4f}  {cam}")

    print(f"\ncamera {camera}")
    for mode in args.modes.split(","):
        out = os.path.join(tmp, f"{mode}.png")
        render(args.bot, out, camera, None if mode == "stored" else mode)
        print(f"  {mode:12} IoU={iou(normalised(foreground(out)), target):.4f}  -> {out}")


if __name__ == "__main__":
    main()
