#!/usr/bin/env python3
"""ensemble_parts.py <dir> [W0 W1]: parts lost and how the fights end, of an
ensemble.sh run made with FIDTRACE_LOG=1 (docs/29 section 12ar).

Reads logW.txt: the duel's log lines ("X: <part> knocked off", "X immobilized",
"X eliminated", "X chassis destroyed", "X: critical hit n") and the final
"fight <s>s, reason ..." line. Prints, per robot, the fights in which it lost
any part, the mean parts lost per fight and the parts by name; the mean fight
length, the share of fights that run to the clock; and how the others end (the
last robot-out line before the end: destroyed, immobilized, eliminated).
"""
import collections
import re
import sys

LINE = re.compile(r"^\[\s*([0-9.]+)\]\s+t=[0-9.]+s\s+(.*)$")


def main():
    d = sys.argv[1]
    w0, w1 = (int(sys.argv[2]), int(sys.argv[3])) if len(sys.argv) > 3 else (0, 47)
    fights = []
    for w in range(w0, w1 + 1):
        try:
            lines = open(f"{d}/log{w}.txt").read().splitlines()
        except OSError:
            continue
        f = {"parts": [], "end": None, "len": 0.0, "reason": "?", "crit": collections.Counter()}
        last_out = None
        for l in lines:
            m = LINE.match(l)
            if m:
                t, text = float(m.group(1)), m.group(2)
                p = re.match(r"(.+?): (.+) knocked off$", text)
                if p:
                    f["parts"].append((t, p.group(1), p.group(2)))
                    continue
                c = re.match(r"(.+?): critical hit", text)
                if c:
                    f["crit"][c.group(1)] += 1
                    continue
                o = re.match(r"(.+?) (immobilized|eliminated|chassis destroyed)$", text)
                if o:
                    last_out = (t, o.group(1), o.group(2))
            r = re.match(r"fight ([0-9.]+)s, reason (\S+)", l)
            if r:
                f["len"], f["reason"] = float(r.group(1)), r.group(2)
        f["end"] = last_out
        fights.append(f)
    n = len(fights)
    if not n:
        sys.exit("no runs in " + d)
    print(f"n={n} mean length {sum(f['len'] for f in fights) / n:.1f} s, "
          f"to the clock {sum(f['reason'].startswith('Some(Timeout') for f in fights)}")
    ends = collections.Counter()
    for f in fights:
        if f["reason"].startswith("Some(Timeout"):
            ends["timeout"] += 1
        elif f["end"]:
            ends[f"{f['end'][1]} {f['end'][2]}"] += 1
        else:
            ends[f["reason"]] += 1
    print("ends: " + ", ".join(f"{k} {v}" for k, v in sorted(ends.items(), key=lambda kv: -kv[1])))
    lens = sorted(f["len"] for f in fights)
    print(f"length quartiles {lens[n // 4]:.0f} / {lens[n // 2]:.0f} / {lens[3 * n // 4]:.0f} s")
    robots = sorted({p[1] for f in fights for p in f["parts"]})
    total = sum(len(f["parts"]) for f in fights)
    print(f"parts lost: {total} in {sum(bool(f['parts']) for f in fights)} fights "
          f"({total / n:.2f} per fight)")
    for r in robots:
        names = collections.Counter(p[2] for f in fights for p in f["parts"] if p[1] == r)
        fl = sum(any(p[1] == r for p in f["parts"]) for f in fights)
        firsts = sorted(min(p[0] for p in f["parts"] if p[1] == r) for f in fights
                        if any(p[1] == r for p in f["parts"]))
        med = firsts[len(firsts) // 2] if firsts else 0
        print(f"  {r}: {fl} fights, {sum(names.values())} parts (first loss median {med:.0f} s): "
              + ", ".join(f"{k} {v}" for k, v in names.most_common()))


if __name__ == "__main__":
    main()
