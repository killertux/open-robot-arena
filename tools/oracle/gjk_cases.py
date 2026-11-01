#!/usr/bin/env python3
"""gjk_cases.py <capture.jsonl>... > cases.txt: the distance queries of a live
`frida/ra2_fightgjk.js` capture as plain text for `examples/gjkreplay.rs`
(docs/29 section 12ak).

    S <shape> <count> <x y z>*count | adj <n> (<len> <i>*len)*n    a hull
    J <shape> <seq> <start> <n> <ia>*n <ib>*n <dist> <xf 12> <tri 9> <onA 3> <after>

A hull's world position is `xf` (3 columns of 3, then the translation).
"""
import json
import math
import sys

shapes = {}
for path in sys.argv[1:]:
    tag = path.split("__")[-1].split(".")[0]
    for line in open(path):
        r = json.loads(line)
        if r[0] == "S":
            key = tag + r[2]
            shapes[key] = r
            adj = r[5] if len(r) > 5 and isinstance(r[5], list) else None
            out = ["S", key, str(r[3])] + [repr(x) for x in r[4]]
            if adj:
                out += ["adj", str(len(adj))]
                for l in adj:
                    out += [str(len(l))] + [str(i) for i in l]
            print(" ".join(out))
        elif r[0] == "J":
            d = r[10]
            if not (math.isfinite(d) and 0 < d < 0.25 and r[7] < 4):
                continue
            print(" ".join(["J", tag + r[16], str(r[1]), str(r[5]), str(r[7])]
                           + [str(i) for i in r[8][:r[7]]] + [str(i) for i in r[9][:r[7]]]
                           + [repr(d)] + [repr(x) for x in r[15][:12]] + [repr(x) for x in r[14]]
                           + [repr(x) for x in r[12]] + [str(r[6])]))
