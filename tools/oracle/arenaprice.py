#!/usr/bin/env python3
"""arenaprice.py <capture.jsonl>...: every arena hit of a live capture against
FUN_0045a5a0 on its own record (docs/29 sections 12o, 12aa).

Reads the `H` rows (the gate 0x417330, player -1: a hit with no attacker) and
the `Q` rows (computeDamage 0x489470 with the record and both sides' rigid
bodies) of frida/ra2_arenahits.js or frida/ra2_hazard.js. A hit priced by a
sub-material (`SetSubMaterialSound`) has no `Q` of its own (the arena's
computeDamage 0x405090 does not call 0x489470), so the record is taken from
any `Q` row of the same contact point nearby. Prints, per capture, how many
hits come to each factor (amount / M) and the largest relative deviation.
"""
import glob
import json
import sys

FACTORS = (0.5, 0.7, 0.8, 0.9, 1.0)


def sub(a, b):
    return [a[i] - b[i] for i in range(3)]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def dot(a, b):
    return sum(a[i] * b[i] for i in range(3))


def rotation_term(body, p, n):
    # body: [ptr, mass, com, inverse inertia +0x10c (3 rows of 4), v, w]
    com, inv = body[2], body[3]
    m = [[inv[0], inv[4], inv[8]], [inv[1], inv[5], inv[9]], [inv[2], inv[6], inv[10]]]
    c = cross(sub(p, com), n)
    return dot(c, [dot(m[i], c) for i in range(3)])


def inverse_mass(m):
    return max(0.01, 0.08 if m <= 0 else 1.0 / m)


def magnitude(rf, sides):
    n, p = rf[1:4], rf[5:8]
    denom = inverse_mass(rf[11]) + inverse_mass(rf[12])
    for s in sides:
        if s and s[2]:
            denom += rotation_term(s[2], p, n)
    return 0.5 * rf[0] / denom


def rows_of(path):
    out = []
    for line in open(path):
        if not line.startswith("["):
            continue
        try:
            out.append(json.loads(line))
        except ValueError:
            pass
    return out


def main():
    paths = [p for a in sys.argv[1:] for p in sorted(glob.glob(a))]
    for path in paths:
        rows = rows_of(path)
        count, worst = {}, {}
        for i, r in enumerate(rows):
            if r[0] != "H" or r[2] != -1 or not r[6] or r[6] <= 0 or not r[7]:
                continue
            for j in range(max(0, i - 6), min(i + 8, len(rows))):
                q = rows[j]
                if q[0] == "Q" and all(abs(q[7][5 + k] - r[7][k]) < 1e-5 for k in range(3)):
                    m = magnitude(q[7], q[8])
                    if m <= 0:
                        break
                    k = r[6] / m
                    f = min(FACTORS, key=lambda x: abs(x - k))
                    count[f] = count.get(f, 0) + 1
                    worst[f] = max(worst.get(f, 0.0), abs(k / f - 1))
                    break
        print(path)
        for f in sorted(count):
            print("  factor %.1f: %d hits, max relative deviation %.1e" % (f, count[f], worst[f]))


if __name__ == "__main__":
    main()
