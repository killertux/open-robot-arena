#!/usr/bin/env python3
"""arena_sources.py: the arena's hits on the robots by source, live against ours
(docs/29 section 12aq).

  arena_sources.py live <name> [ours <ensemble dir>]

<name> is e.g. sentinel-wideload-compressor: every
`arenahits-<name>__live__*.jsonl` of local/oracle/original (or $ORACLE) and,
for the older Compressor fights, `fight-<name>__live__*.jsonl` (no Q rows, so
no price factor: those are classified by where the robot was).

A live hit is classified by the price factor of 12o / 12aa (`amount / M` with
M from the Q row of the same contact point, arenaprice.py): 0.9 the smash arms,
0.7 the spikes, 0.5 the Arenium of the floor and the walls. A hit with no Q
row (and the fight-* captures) falls back to the region: the Compressor's
smash-arm alley is |z| > 16.3 and |x| < 6.5. Ours has the struck piece
(fidtrace's `piece`): `Smash Arm *`, `Spike*_Zone`, else the walls and floor.

Per source and robot: the hits of 10 hp or more (the applied ones) as hp per
fight (mean +- standard error), the share of fights with any, the median and
the largest fight, and the hp per hit.
"""
import glob
import json
import math
import os
import statistics
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import arenaprice  # noqa: E402

ORACLE = os.environ.get("ORACLE", os.path.join(os.path.dirname(__file__), "..", "..", "local", "oracle", "original"))
SOURCES = ("smash", "spikes", "floor/walls")


def alley(p):
    return abs(p[2]) > 16.3 and abs(p[0]) < 6.5


def live_source(rows, i, r):
    """The source of the H row `r` at index i."""
    for j in range(max(0, i - 6), min(i + 8, len(rows))):
        q = rows[j]
        if q[0] == "Q" and len(q) > 8 and all(abs(q[7][5 + k] - r[7][k]) < 1e-5 for k in range(3)):
            m = arenaprice.magnitude(q[7], q[8])
            if m > 0:
                f = min(arenaprice.FACTORS, key=lambda x: abs(x - r[6] / m))
                return {0.9: "smash", 0.7: "spikes"}.get(f, "floor/walls")
    return "smash" if alley(r[7]) else "floor/walls"


def live_fights(name):
    out = []
    for prefix in ("arenahits", "fight"):
        for path in sorted(glob.glob(os.path.join(ORACLE, f"{prefix}-{name}__live__*.jsonl"))):
            rows = []
            for line in open(path):
                try:
                    rows.append(json.loads(line))
                except ValueError:
                    pass
            f = {(v, s): [] for v in (0, 1) for s in SOURCES}
            for i, r in enumerate(rows):
                if r[0] == "H" and r[2] == -1 and r[4] in (0, 1) and r[6] and r[6] >= 10 and r[7]:
                    src = live_source(rows, i, r) if prefix == "arenahits" else ("smash" if alley(r[7]) else "floor/walls")
                    f[(r[4], src)].append(r[6])
            out.append(f)
    return out


def ours_fights(d):
    out = []
    w = 0
    while os.path.exists(f"{d}/h{w}.jsonl"):
        f = {(v, s): [] for v in (0, 1) for s in SOURCES}
        for line in open(f"{d}/h{w}.jsonl"):
            h = json.loads(line)
            if h["att"] != -1 or h["amount"] < 10:
                continue
            piece = h["piece"]
            src = "smash" if piece.startswith("Smash") else "spikes" if piece.startswith("Spike") else "floor/walls"
            f[(h["vic"], src)].append(h["amount"])
        out.append(f)
        w += 1
    return out


def report(label, fights):
    n = len(fights)
    print(f"{label}: {n} fights")
    for v in (0, 1):
        tot = [sum(sum(f[(v, s)]) for s in SOURCES) for f in fights]
        line = f"  robot {v}: total {statistics.mean(tot):7.1f} +- {statistics.pstdev(tot) / math.sqrt(n):6.1f} median {statistics.median(tot):6.1f}"
        print(line)
        for s in SOURCES:
            hp = [sum(f[(v, s)]) for f in fights]
            hits = [len(f[(v, s)]) for f in fights]
            with_any = sum(1 for h in hits if h)
            per_hit = sum(hp) / max(1, sum(hits))
            print(
                f"    {s:12s} {statistics.mean(hp):7.1f} +- {statistics.pstdev(hp) / math.sqrt(n):6.1f} hp/fight, "
                f"{with_any:2d} of {n} fights, median {statistics.median(hp):6.1f}, max {max(hp):7.1f}, {per_hit:5.1f} hp/hit"
            )


def main():
    a = sys.argv[1:]
    if len(a) < 2 or a[0] != "live":
        sys.exit(__doc__)
    report("live", live_fights(a[1]))
    if len(a) >= 4 and a[2] == "ours":
        report("ours", ours_fights(a[3]))


if __name__ == "__main__":
    main()
