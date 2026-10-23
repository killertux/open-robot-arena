#!/usr/bin/env python3
"""hazstate.py <hazard-*.jsonl> <outdir>: an examples/twoprobe state file per
placement trial of a capture_hazard.py capture (docs/29 section 12z): every
robot body's live placed state (the `X` 'place' row of frida/ra2_twobody.js),
with its robot and mass, so twoprobe places ours as live did.
"""
import json
import os
import sys


def main():
    rows = [json.loads(l) for l in open(sys.argv[1])]
    out = sys.argv[2]
    os.makedirs(out, exist_ok=True)
    rest = [r for r in rows if r[0] == "REST"][0]
    ptrs, nscout = rest[2], rest[3]
    mass = {}
    for r in rows:
        if r[0] == "TB" and r[1] == "P":
            mass[r[3]] = r[4][0x190 // 4]
    # The placement agent's rows follow the hazard agent's, in its own order:
    # its `place` rows are the trials in turn.
    trial = -1
    for r in rows:
        if r[0] == "TB" and r[1] == "X" and r[3] == "place":
            trial += 1
            with open(os.path.join(out, "t%02d.state" % trial), "w") as f:
                for (p, c, q, v, w) in r[4]:
                    rb = 0 if ptrs.index(p) < nscout else 1
                    f.write(" ".join(map(str, [rb, mass[p]] + c + q + v + w)) + "\n")
            print("trial", trial, "placed", len(r[4]), "bodies")


if __name__ == "__main__":
    main()
