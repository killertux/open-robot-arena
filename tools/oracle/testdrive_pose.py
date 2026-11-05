#!/usr/bin/env python3
"""Compare the practice robot's pose over time, live against ours (docs/36 9.32).

    testdrive_pose.py LIVE_SAMPLE OURS_RAW

LIVE_SAMPLE is what `ra2_python.py --sample "repr([(p, plus.getLocation(p), plus.getHeading(p), plus.getSpeed(p)) for p in
plus.getPlayers()]) + ..."` wrote while `ra2_drive.py -f routes/botlab/06-testdrive.steps` drove the Test Robot (game time from
the tab click). OURS_RAW is the output of `LABSHOT_POSE=1 labshot -f ...` on the same route: `labshot` runs the practice arena
through every `wait` and through the driver's own time (a click 0.25 s, a `hide` 0.13, a shot 1.86, a held key 0.1) and prints
`pose T X Y Z h HEADING` every 0.1 s. Prints both poses with time counted from the first sample the heading moved: the live
game's own run-to-run spread (frame time) is about a metre after three seconds, ours is deterministic.
"""
import bisect
import re
import sys


def load_live(path):
    rows = []
    for line in open(path):
        m = re.match(r"([\d.]+)\t\[\(0, \(([^,]+), ([^,]+), ([^)]+)\), ([^,]+), ([^)]+)\)\]", line)
        if m:
            r = tuple(float(x) for x in m.groups())
            if not rows or r[0] > rows[-1][0] + 1e-6:
                rows.append(r)
    return rows


def load_ours(path):
    rows = []
    for line in open(path):
        if line.startswith("pose"):
            p = line.split()
            rows.append((float(p[1]), float(p[2]), float(p[4]), float(p[6])))
    return rows


def start(rows, heading):
    for r in rows:
        if r[0] > 7 and abs(r[heading]) > 0.05:
            return r[0]
    return 0.0


def at(rows, t, i):
    k = max(0, min(len(rows) - 1, bisect.bisect([r[0] for r in rows], t) - 1))
    return rows[k][i]


def main():
    live, ours = load_live(sys.argv[1]), load_ours(sys.argv[2])
    tl, to = start(live, 4), start(ours, 3)
    print(f"heading > 0.05 at live {tl:.2f} s, ours {to:.2f} s")
    print("   dt   live x     z      h     ours x     z      h")
    t = 0.0
    while t < 5.01:
        lx, lz, lh = at(live, tl + t, 1), at(live, tl + t, 3), at(live, tl + t, 4)
        ox, oz, oh = at(ours, to + t, 1), at(ours, to + t, 2), at(ours, to + t, 3)
        print(f"{t:5.2f} {lx:7.2f} {lz:7.2f} {lh:6.2f} {ox:8.2f} {oz:7.2f} {oh:6.2f}")
        t += 0.5


if __name__ == "__main__":
    main()
