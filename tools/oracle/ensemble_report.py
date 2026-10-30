#!/usr/bin/env python3
"""ensemble_report.py <dir> [W0 W1]: summarise an ensemble of ensemble.sh.

Per robot (0 = the first bot named, 1 = the second): the hits it landed on the
other robot that reached 10 hp ("R<n>n", the docs' counted hits), and the
arena's hits on it that reached 10 hp (A<n>n count, A<n>hp hp, and by hazard
zone of the box: saw, spikes 1 and 2), as mean +- standard error per fight,
with the mean length and the number of full-length fights (docs/29 12ag).
"""
import collections
import json
import math
import sys

ZONES = {"spk1": (-3.7, -5.2), "spk2": (4.9, -5.2), "saw": (-0.1, 5.4)}


def zone(p):
    for k, (x, z) in ZONES.items():
        if abs(p[0] - x) < 1.6 and abs(p[2] - z) < 1.8 and p[1] > -2.6:
            return k
    return "other"


def main():
    d = sys.argv[1]
    w0, w1 = (int(sys.argv[2]), int(sys.argv[3])) if len(sys.argv) > 3 else (0, 47)
    rows = []
    for w in range(w0, w1 + 1):
        try:
            hits = [json.loads(l) for l in open(f"{d}/h{w}.jsonl")]
            log = [l for l in open(f"{d}/log{w}.txt") if l.startswith("fight")]
        except OSError:
            continue
        r = collections.defaultdict(float)
        for h in hits:
            if h["amount"] < 10:
                continue
            if h["att"] == -1:
                r["A%dn" % h["vic"]] += 1
                r["A%dhp" % h["vic"]] += h["amount"]
                r["A%dhp_%s" % (h["vic"], zone(h["p"]))] += h["amount"]
            else:
                r["R%dn" % h["att"]] += 1
        r["len"] = float(log[0].split()[1].rstrip("s,")) if log else 0.0
        rows.append(r)
    n = len(rows)
    if n < 2:
        sys.exit("no runs in " + d)
    keys = sorted({k for r in rows for k in r if k != "len"})

    def ms(k):
        x = [r[k] for r in rows]
        m = sum(x) / n
        s = (sum((a - m) ** 2 for a in x) / (n - 1)) ** 0.5 / math.sqrt(n)
        return f"{m:.1f}+-{s:.1f}"

    full = sum(r["len"] > 119.9 for r in rows)
    print(f"n={n} mean length {sum(r['len'] for r in rows) / n:.0f} s, full length {full}")
    print(" | ".join(f"{k} {ms(k)}" for k in keys))


if __name__ == "__main__":
    main()
