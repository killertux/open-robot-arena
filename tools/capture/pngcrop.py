#!/usr/bin/env python3
"""Crop (and optionally nearest-neighbour scale) a PNG, standard library only.

    tools/capture/pngcrop.py in.png out.png --crop X Y W H
    tools/capture/pngcrop.py in.png out.png --crop 0 10 1280 1005 --scale 1024 768

The capture harness uses this to remove the black letterbox bars a fullscreen
window adds around the shell's 1280x1005 scene before comparing it with an
offscreen render of that scene.  It is deliberately not a general image editor:
one rectangle, nearest-neighbour scaling, no resampling filter that would hide
a one-pixel layout difference.

The PNG reader/writer are `tools/oracle/compare.py`'s, so the tool handles the
same 8-bit PNGs the rest of the pipeline writes and avoids a second decoder.
"""

import argparse
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "oracle"))
from compare import read_png, write_png  # noqa: E402


def main(argv):
    parser = argparse.ArgumentParser(description="Crop and/or scale a PNG.")
    parser.add_argument("source")
    parser.add_argument("target")
    parser.add_argument(
        "--crop", nargs=4, type=int, metavar=("X", "Y", "W", "H"), required=True
    )
    parser.add_argument(
        "--scale", nargs=2, type=int, metavar=("W", "H"), help="nearest-neighbour"
    )
    args = parser.parse_args(argv)

    width, height, rgb = read_png(args.source)
    x, y, w, h = args.crop
    if w <= 0 or h <= 0:
        parser.error("crop width and height must be positive")
    if x < 0 or y < 0 or x + w > width or y + h > height:
        parser.error(f"crop {x},{y} {w}x{h} is outside {width}x{height}")

    out = bytearray(w * h * 3)
    for row in range(h):
        src = ((y + row) * width + x) * 3
        dst = row * w * 3
        out[dst : dst + w * 3] = rgb[src : src + w * 3]
    out_w, out_h = w, h

    if args.scale:
        out_w, out_h = args.scale
        if out_w <= 0 or out_h <= 0:
            parser.error("scale width and height must be positive")
        scaled = bytearray(out_w * out_h * 3)
        for row in range(out_h):
            sy = min(h - 1, row * h // out_h)
            for col in range(out_w):
                sx = min(w - 1, col * w // out_w)
                src = (sy * w + sx) * 3
                dst = (row * out_w + col) * 3
                scaled[dst : dst + 3] = out[src : src + 3]
        out = scaled

    write_png(args.target, out_w, out_h, out)
    print(f"{args.source} -> {args.target} ({w}x{h} crop, {out_w}x{out_h})")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (OSError, ValueError) as error:
        print(f"pngcrop: {error}", file=sys.stderr)
        sys.exit(2)
