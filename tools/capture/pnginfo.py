#!/usr/bin/env python3
"""Report a PNG's size and whether it is uniform, for the capture assertions.

    tools/capture/pnginfo.py frame.png [--expect 1280x1024] [--nonuniform]
                                       [--label menu] [--json]

The smoke harness must not accept a black window or an empty file as a
capture.  This prints the dimensions, the per-channel mean and standard
deviation, the darkest and brightest sample, and how many distinct RGB colours
were seen (capped).  `--expect` and `--nonuniform` turn those into assertions:
a wrong size or a frame whose standard deviation is at or below 0.01 exits 2.

Only the standard library; the PNG decoder is `tools/oracle/compare.py`'s.
"""

import argparse
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "oracle"))
from compare import read_png  # noqa: E402


def main(argv):
    parser = argparse.ArgumentParser(description="Inspect a PNG frame.")
    parser.add_argument("image")
    parser.add_argument("--expect", metavar="WxH")
    parser.add_argument(
        "--nonuniform",
        action="store_true",
        help="fail when every pixel is identical",
    )
    parser.add_argument("--label", default="")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    width, height, rgb = read_png(args.image)
    pixels = width * height
    total = [0, 0, 0]
    squares = [0, 0, 0]
    minimum, maximum = 255, 0
    colours = set()
    for i in range(pixels):
        base = i * 3
        for channel in range(3):
            value = rgb[base + channel]
            total[channel] += value
            squares[channel] += value * value
            if value < minimum:
                minimum = value
            if value > maximum:
                maximum = value
        if len(colours) <= 4096:
            colours.add(bytes(rgb[base : base + 3]))

    means = [t / pixels for t in total]
    stdev = [
        math.sqrt(max(0.0, squares[c] / pixels - means[c] * means[c]))
        for c in range(3)
    ]
    overall = sum(means) / 3.0
    overall_sd = sum(stdev) / 3.0
    record = {
        "label": args.label,
        "file": args.image,
        "width": width,
        "height": height,
        "mean": means,
        "stdev": stdev,
        "mean_all": overall,
        "stdev_all": overall_sd,
        "min": minimum,
        "max": maximum,
        "colours": len(colours),
        "uniform": overall_sd <= 0.01,
    }
    if args.json:
        print(json.dumps(record, sort_keys=True))
    else:
        print(
            f"{args.label + ': ' if args.label else ''}{width}x{height}, "
            f"mean {overall:.2f}, stdev {overall_sd:.2f}, "
            f"range {minimum}..{maximum}, colours {len(colours)}"
            f"{'+' if len(colours) > 4096 else ''}"
        )

    failed = False
    if args.expect:
        try:
            want_w, want_h = (int(v) for v in args.expect.lower().split("x"))
        except ValueError:
            parser.error(f"--expect wants WxH, got {args.expect!r}")
        if (width, height) != (want_w, want_h):
            print(
                f"pnginfo: {args.image} is {width}x{height}, expected "
                f"{want_w}x{want_h}",
                file=sys.stderr,
            )
            failed = True
    if args.nonuniform and record["uniform"]:
        print(
            f"pnginfo: {args.image} is uniform (stdev {overall_sd:.3f}); "
            "this is not a drawn frame",
            file=sys.stderr,
        )
        failed = True
    return 2 if failed else 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (OSError, ValueError) as error:
        print(f"pnginfo: {error}", file=sys.stderr)
        sys.exit(2)
