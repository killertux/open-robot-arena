#!/usr/bin/env python3
"""inversion_events.py <ensemble dir> [robot=0]: what turns a robot over (docs/29 12am).

For the output of ensemble.sh run with FIDTRACE_UP=1: every run in which the robot
lies on its back (up.y < 0) for 1.5 s or more; for the first such spell the time it
left upright (last up.y > 0.7 before it), the time it first reached up.y < 0, and
the hits taken (priced, h<w>.jsonl) in the 1.5 s before the turn is complete.
Prints one line per run and a tally by the largest hit before the turn.
"""
import ast, json, os, sys, glob, collections

d = sys.argv[1]
rob = int(sys.argv[2]) if len(sys.argv) > 2 else 0
tally = collections.Counter()
n = 0
for tp in sorted(glob.glob(f"{d}/t*.tsv"), key=lambda p: int(os.path.basename(p)[1:-4])):
    w = int(os.path.basename(tp)[1:-4])
    ts, up, pos = [], [], []
    for line in open(tp).read().splitlines()[1:]:
        a, b = line.split("\t", 1)
        try:
            rows = ast.literal_eval(b)
        except (ValueError, SyntaxError):
            continue
        ts.append(float(a)); up.append(rows[rob][5]); pos.append(rows[rob][1])
    n += 1
    start = None; spell = None
    for i, t in enumerate(ts):
        if up[i] < 0:
            if start is None:
                start = i
            if t - ts[start] >= 1.5:
                spell = start
                break
        else:
            start = None
    if spell is None:
        continue
    # turn begins: last upright (>0.7) before spell
    j = spell
    while j > 0 and up[j] < 0.7:
        j -= 1
    t_up, t_inv = ts[j], ts[spell]
    hits = []
    hp = f"{d}/h{w}.jsonl"
    if os.path.exists(hp):
        for line in open(hp):
            h = json.loads(line)
            if h["vic"] == rob and t_up - 1.0 <= h["t"] <= t_inv and h["amount"] > 0:
                hits.append(h)
    ymax = max(pos[k][1] for k in range(len(ts)) if t_up - 1.0 <= ts[k] <= t_inv)
    big = max(hits, key=lambda h: h["amount"], default=None)
    key = "none" if big is None else f'{"arena:"+big["piece"] if big["att"]==-1 else "robot"}:{big["comp"]}'
    air = ymax > 1.2
    tally[('air:' if air else 'ground:') + key] += 1
    print(f"w{w:2d} upright until {t_up:6.2f} on back {t_inv:6.2f} ({t_inv-t_up:4.2f}s) ymax {ymax:4.1f} at ({pos[spell][0]:.1f},{pos[spell][2]:.1f}); big hit: {key} {0 if big is None else big['amount']:.0f} @{0 if big is None else big['t']:.2f}; hits "
          + " ".join(f'{h["t"]:.2f}:{"A" if h["att"]==-1 else h["att"]}:{h.get("piece","")[:6]}:{h["comp"][:5]}:{h["amount"]:.0f}' for h in hits[-6:]))
print(f"{sum(tally.values())} of {n} runs on back >= 1.5 s")
for k, v in tally.most_common():
    print(f"  {v:3d} {k}")
