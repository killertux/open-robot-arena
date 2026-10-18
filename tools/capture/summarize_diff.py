#!/usr/bin/env python3
"""Summarise one `tools/oracle/compare.py` JSON record for the A/B table.

    python3 tools/capture/summarize_diff.py diff.json

Prints four tab-separated columns: `mae`, `pointer-masked mae`, the number of
16x16 blocks above 20, and the five largest blocks as `(x,y)=value`.  The block
coordinates are the top-left pixel of the block in the compared frame, so a
reader can find the region in the diff image.  A null `pointer` (no OS cursor
was detected) is reported as `-`, not as zero.
"""

import json
import sys


def main(argv):
    if len(argv) != 1:
        print("usage: summarize_diff.py diff.json", file=sys.stderr)
        return 2
    with open(argv[0]) as f:
        record = json.load(f)
    mae = record["mae"]["all"]
    pointer = record.get("pointer") or {}
    masked = ((pointer.get("stats_without") or {}).get("mae") or {}).get("all")
    heat = record["heatmap"]
    values = heat["values"]
    block = heat["block"]
    flat = []
    over = 0
    for row, line in enumerate(values):
        for col, value in enumerate(line):
            flat.append((value, col * block, row * block))
            if value > 20:
                over += 1
    flat.sort(reverse=True)
    top = " ".join(f"({x},{y})={value:.1f}" for value, x, y in flat[:5])
    print(
        f"{mae:.4f}\t{'-' if masked is None else f'{masked:.4f}'}\t"
        f"{over}\t{top}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
