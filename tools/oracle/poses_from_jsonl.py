#!/usr/bin/env python3
"""Print a capture_shape.py jsonl's poses as RA2_SHAPE_POSES ("x,y,z,heading;...", docs/34 section 11)."""
import ast
import json
import sys

out = []
for line in open(sys.argv[1]):
    r = json.loads(line)
    if r.get("ev") == "shot":
        a, h = ast.literal_eval(r["pose"])[:2]
        out.append("%.6f,%.6f,%.6f,%.6f" % (a[0], a[1], a[2], h))
print(";".join(out))
