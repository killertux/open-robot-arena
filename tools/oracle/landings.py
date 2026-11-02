#!/usr/bin/env python3
"""landings.py <ensemble dir> [n]: a thrown robot (centre above 1.5 m) and how it lands (docs/29 12am).

Per flight (centre y > 1.5 m, then back below 0.7 m): peak height, whether it landed inside the
arena (|x|,|z| < 14) and the up.y 1.5 s after the landing. FIDTRACE_UP=1 FIDTRACE_COM=1 traces.
"""
import ast, sys, collections
d = sys.argv[1]; n = int(sys.argv[2]) if len(sys.argv) > 2 else 48
res = collections.defaultdict(list)
for w in range(n):
    rows = []
    for line in open(f"{d}/t{w}.tsv").read().splitlines()[1:]:
        a, b = line.split("\t", 1)
        try:
            rows.append((float(a), ast.literal_eval(b)))
        except (ValueError, SyntaxError):
            pass
    for r in (0, 1):
        i = 0
        while i < len(rows):
            if rows[i][1][r][1][1] > 1.5:
                j = i; peak = 0
                while j < len(rows) and rows[j][1][r][1][1] > 0.7:
                    peak = max(peak, rows[j][1][r][1][1]); j += 1
                if j >= len(rows): break
                x, y, z = rows[j][1][r][1]
                k = j
                while k < len(rows) and rows[k][0] < rows[j][0] + 1.5: k += 1
                k = min(k, len(rows) - 1)
                up = rows[k][1][r][5]
                res[r].append((rows[i][0], peak, abs(x) < 14 and abs(z) < 14 and y > -1, up))
                i = j
            i += 1
for r in (0, 1):
    L = res[r]; inside = [l for l in L if l[2]]
    print(f"robot {r}: {len(L)} flights, {len(inside)} landing inside, on its back 1.5 s after: {sum(l[3] < 0 for l in inside)}; "
          f"peaks median {sorted(l[1] for l in L)[len(L)//2]:.1f}" if L else f"robot {r}: none")
