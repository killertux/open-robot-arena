#!/usr/bin/env python3
"""Summarise a fight capture (capture_fight.py): the priced hits.

usage: fight_report.py <fight-*.jsonl> [--list]

Pairs each gate call (0x417330, H rows) with the computeDamage call (P row)
just before it, and prints the hit count, the counted hits (>= 10), the time
of the first counted hit and the amount distribution, per attacker player.
"""
import json
import sys


def hits(path):
    t, last_p, out = 0.0, None, []
    for line in open(path):
        r = json.loads(line)
        if r[0] == 'A':
            t += r[2]
        elif r[0] == 'P':
            last_p = r
        elif r[0] == 'H':
            speed = last_p[3][0] if last_p else None
            out.append({'t': t, 'att': r[2], 'att_comp': r[3], 'vic': r[4], 'vic_comp': r[5],
                        'amount': r[6], 'speed': speed, 'counted': r[6] >= 10.0})
    return out


def main():
    hs = hits(sys.argv[1])
    print(f"{len(hs)} priced hits (amount != 0 reaches the gate), {sum(h['counted'] for h in hs)} counted (>= 10)")
    for att in sorted({h['att'] for h in hs}):
        mine = [h for h in hs if h['att'] == att]
        c = [h for h in mine if h['counted']]
        a = sorted(h['amount'] for h in mine)
        q = lambda f: a[min(len(a) - 1, int(f * len(a)))] if a else 0
        print(f" attacker {att}: {len(mine)} hits, {len(c)} counted, total counted {sum(h['amount'] for h in c):.1f};"
              f" amount p50/p90/max {q(.5):.2f}/{q(.9):.2f}/{max(a) if a else 0:.2f};"
              f" first counted at {c[0]['t']:.2f} s" if c else f" attacker {att}: {len(mine)} hits, none counted;"
              f" amount p50/p90/max {q(.5):.2f}/{q(.9):.2f}/{max(a) if a else 0:.2f}")
    if '--list' in sys.argv:
        for h in hs:
            if h['amount'] >= 3:
                print(f"  {h['t']:7.2f} {h['att']}->{h['vic']} amount {h['amount']:7.2f} speed {h['speed']}")


if __name__ == '__main__':
    main()
