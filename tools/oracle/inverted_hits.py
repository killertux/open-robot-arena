#!/usr/bin/env python3
"""inverted_hits.py: the arena's hits on a robot, split by whether it is on its
back (docs/29 section 12al).

  inverted_hits.py live <name> [seq | hits_seq:pos_seq ...]
      <name> is e.g. scout-alarm-octagon: pairs every
      {arenahits,alarmfight}-<name>__live__<seq>.jsonl that has a
      fightpos-<name>__live__<seq>.tsv beside it (the same session numbers),
      or only the given seqs; `0:4` pairs arenahits 000 with fightpos 004
      (the Compressor's fightpos numbers run on from an older capture), in
      local/oracle/original (or $ORACLE).
  Only the fight itself counts (up to the moment both brains go idle); ALL=1
  in the environment keeps the idle tail of the capture too.
  inverted_hits.py ours <ensemble dir> [n]
      the output of ensemble.sh run with FIDTRACE_UP=1 (t<w>.tsv carries the
      chassis' up.y in its last field, h<w>.jsonl the priced hits).

For each robot: the share of fights in which it lay on its back for 1.5 s or
more, the share of fight time on its back, and the arena hits of 10 hp or more
(the applied ones) per second on its back and upright, with the hp per fight.
"""
import ast
import bisect
import glob
import json
import os
import sys

ACTIVE = os.environ.get("ALL") is None  # ALL=1: also the idle tail of a live capture
ORACLE = os.environ.get("ORACLE", os.path.join(os.path.dirname(__file__), "..", "..", "local", "oracle", "original"))


def live_fight(path_hits, path_pos):
    t, hits = 0.0, []
    for line in open(path_hits):
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if r[0] == "A":
            t += r[2]
        elif r[0] == "H" and r[2] == -1 and r[6] > 0:
            hits.append((t, r[4], r[6]))
    ts, up = [], []
    for line in open(path_pos):
        if line.startswith("time"):
            continue
        a, b = line.split("\t", 1)
        try:
            rows = ast.literal_eval(b.strip())
        except (ValueError, SyntaxError):
            continue
        t = float(a)
        # The fight is over when both brains fall idle (last tactic `None` or
        # `-`) after the opening: live the positions then stand still and the
        # robots drift (a robot in a pit is "on its back" from there).
        if ACTIVE and t > 8.0 and all(x[3] in ("None", "-") for x in rows):
            break
        ts.append(t)
        up.append({x[0]: bool(x[2]) for x in rows})
    return hits, ts, up


def ours_fight(d, w):
    ts, up = [], []
    for line in open(f"{d}/t{w}.tsv").read().splitlines()[1:]:
        a, b = line.split("\t", 1)
        try:
            rows = ast.literal_eval(b)
        except (ValueError, SyntaxError):
            continue
        ts.append(float(a))
        up.append({i: r[5] < 0 for i, r in enumerate(rows)})
    hits = []
    for line in open(f"{d}/h{w}.jsonl"):
        h = json.loads(line)
        if h["att"] == -1:
            hits.append((h["t"], h["vic"], h["amount"]))
    return hits, ts, up


def report(fights, label):
    n = len(fights)
    agg = {}
    for hits, ts, up in fights:
        if len(ts) < 2:
            continue
        for r in (0, 1):
            a = agg.setdefault(r, dict(T=0.0, Tinv=0.0, fights_inv=0, hp={True: 0.0, False: 0.0}, n10={True: 0, False: 0}))
            start, long_inv = None, False
            for i in range(1, len(ts)):
                dt = ts[i] - ts[i - 1]
                a["T"] += dt
                if up[i].get(r):
                    a["Tinv"] += dt
                    if start is None:
                        start = ts[i - 1]
                    if ts[i] - start >= 1.5:
                        long_inv = True
                else:
                    start = None
            a["fights_inv"] += long_inv
            for t, vic, amount in hits:
                if vic != r or amount < 10 or not ts[0] <= t <= ts[-1]:
                    continue
                i = min(bisect.bisect_left(ts, t), len(ts) - 1)
                inv = bool(up[i].get(r))
                a["hp"][inv] += amount
                a["n10"][inv] += 1
    print(f"{label}: {n} fights")
    for r, a in sorted(agg.items()):
        up_t = a["T"] - a["Tinv"]
        print(
            f"  robot {r}: on its back >= 1.5 s in {a['fights_inv']} of {n}; {100 * a['Tinv'] / a['T']:.1f}% of {a['T'] / n:.0f} s/fight;"
            f" hits >= 10: on back {a['n10'][True] / n:.2f}/fight, {a['hp'][True] / max(a['Tinv'], 1e-9):.2f} hp/s"
            f" ({a['hp'][True] / n:.1f} hp/fight); upright {a['n10'][False] / n:.2f}/fight, {a['hp'][False] / max(up_t, 1e-9):.2f} hp/s"
            f" ({a['hp'][False] / n:.1f} hp/fight)"
        )


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    if sys.argv[1] == "live":
        name = sys.argv[2]
        pairs = []
        for tok in sys.argv[3:]:
            h, _, p = tok.partition(":")
            pairs.append((int(h), int(p or h)))
        if not pairs:
            for pos in sorted(glob.glob(f"{ORACLE}/fightpos-{name}__live__*.tsv")):
                seq = int(pos.rsplit("__", 1)[1][:3])
                pairs.append((seq, seq))
        fights = []
        for h, p in pairs:
            pos = f"{ORACLE}/fightpos-{name}__live__{p:03d}.tsv"
            for kind in ("arenahits", "alarmfight", "boxfight"):
                path = f"{ORACLE}/{kind}-{name}__live__{h:03d}.jsonl"
                if os.path.exists(path) and os.path.exists(pos):
                    fights.append(live_fight(path, pos))
                    break
        report(fights, f"live {name}")
    else:
        d = sys.argv[2]
        n = int(sys.argv[3]) if len(sys.argv) > 3 else 48
        report([ours_fight(d, w) for w in range(n) if os.path.exists(f"{d}/h{w}.jsonl")], f"ours {d}")


if __name__ == "__main__":
    main()
