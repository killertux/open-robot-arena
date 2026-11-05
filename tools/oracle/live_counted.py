#!/usr/bin/env python3
"""live_counted.py <partsfight-*.jsonl>...: counted hits (priced >= 10 hp, the 0x417330 gate) of
live fights captured with frida/ra2_parts.js, per attacker (0 = player 0, -1 = the arena), within
the first 120 s of the match clock, with the time and size of each (docs/29 section 12bb)."""
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import parts_live as pl  # noqa: E402


def run(paths, cap=120.0):
    out = []
    for p in paths:
        r = pl.analyse(p)
        off = r["off"]
        cnt = collections.Counter()
        rows = []
        for t, row in pl.load(p):
            if row[0] == "H" and t - off <= cap and row[6] >= 10:
                cnt[row[2]] += 1
                rows.append((round(t - off, 1), row[2], round(row[6])))
        out.append((os.path.basename(p), dict(cnt), r["out"][:1], rows))
    return out


if __name__ == "__main__":
    for x in run(sys.argv[1:]):
        print(x)
