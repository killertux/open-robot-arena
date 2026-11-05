#!/usr/bin/env python3
"""zone_replay.py <zonepose-*.jsonl>... > samples.txt  (docs/29 12av)

Turns the P rows of frida/ra2_zonepose.js (every phantom-pair event of the original, with the two
bodies' centre of mass and rotation) into the sample lines `examples/zoneprobe` reads:
`t live_ab live_ba comA(3) rotA(9) comB(3) rotB(9)`.  Only the events between a robot's SmartZone (a phantom
tagged 16 / 32) and the other robot's chassis collidable (tag 17 / 33) are used: an enter says the
chassis overlaps the zone at those poses (1), a leave says it does not (0).  A is robot 1 (the first bot),
B robot 2.  `live_ab` is whether A's zone holds B's chassis, `live_ba` the other way, -1 for the direction
the event does not speak about.

  zoneprobe <arena.py> <botA> <botB> samples.txt
"""
import json
import sys


def main():
    for path in sys.argv[1:]:
        for line in open(path):
            r = json.loads(line)
            if r[0] != "P":
                continue
            _, _, t, leaving, a, b = r
            if not a or not b or len(a) < 7 or len(b) < 7:
                continue
            for x, y in ((a, b), (b, a)):
                if x[3] == 1 and x[2] in (16, 32) and y[3] == 0 and y[2] in (17, 33):
                    owner, other = x[2] >> 4, y[2] >> 4
                    if owner == other:
                        continue
                    live = 0 if leaving else 1
                    ab, ba = (live, -1) if owner == 1 else (-1, live)
                    pa, pb = (x, y) if owner == 1 else (y, x)
                    vals = [t, ab, ba] + pa[4] + pa[5] + pb[4] + pb[5]
                    print(" ".join(repr(v) for v in vals))


main()
