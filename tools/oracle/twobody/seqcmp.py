# seqcmp.py <live.jsonl> <oursdir> [trials] [-v]: the contacts the resolver gets, live against ours,
# substep by substep (first piece): the pair sequence (bodies, orientation) and each pair's point order.
# Prints per trial the first substep where the pair sequence, the A/B orientation or the point order differs.
import sys, os, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *
args = [a for a in sys.argv[1:] if not a.startswith('-')]
verbose = '-v' in sys.argv
meta, who, trials = load_live(args[0])
D = args[1]
sel = [int(x) for x in args[2].split(',')] if len(args) > 2 else sorted(trials)
names = {(0, 0): 'Sch', (1, 0): 'Ach'}
for k, bb in enumerate(meta['scout'][1:], 1): names[(0, k)] = 'Sfork' if bb[1] == 26 else 'Swh%d' % k
for k, bb in enumerate(meta['alarm'][1:], 1): names[(1, k)] = 'Arod' if bb[1] == 33 else 'Awh%d' % k
def lab(m, h):
    k = m.get(h)
    return names[k] if k else 'W'
def seq(R, m):
    out = []
    for c in R or []:
        out.append((lab(m, c[0]), lab(m, c[1]), c[3], c[2], c[4]))
    return out
def groups(s):
    g = []
    for a, b, p, n, d in s:
        key = (a, b)
        if g and g[-1][0] == key: g[-1][1].append((p, n, d))
        else: g.append((key, [(p, n, d)]))
    return g
def canon(k): return tuple(sorted(k))
res = {}
for i in sel:
    f = os.path.join(D, 't%02d.state' % i)
    if not os.path.exists(f + '.dump'): continue
    ow = {}
    for l in open(f + '.out'):
        p = l.split()
        if p and p[0] == 'MAP' and p[2] != '-': ow[p[3]] = (int(p[1]), int(p[2]))
    L = per_step(trials[i]['rows'], who)
    O = per_step([json.loads(l) for l in open(f + '.dump')], ow)
    first = {}
    for n in sorted(L):
        if n < 0 or n not in O: continue
        gl, go = groups(seq(L[n]['R'], who)), groups(seq(O[n]['R'], ow))
        pl, po = [canon(k) for k, _ in gl], [canon(k) for k, _ in go]
        if pl != po:
            if sorted(pl) == sorted(pl) and sorted(pl) == sorted(po):
                first.setdefault('seq', n)
                if verbose and first['seq'] == n:
                    print('t%02d n%d sequence\n  live %s\n  ours %s' % (i, n, [(k, len(v)) for k, v in gl], [(k, len(v)) for k, v in go]))
            first.setdefault('pairs', n)
            if verbose and first['pairs'] == n:
                print('t%02d n%d pairs\n  live %s\n  ours %s' % (i, n, [(k, len(v)) for k, v in gl], [(k, len(v)) for k, v in go]))
            continue
        for (kl, vl), (ko, vo) in zip(gl, go):
            if kl != ko: first.setdefault('orient', (n, kl, ko))
            # point order: each live point's nearest of ours
            if len(vl) == len(vo) and len(vl) > 1:
                perm = [min(range(len(vo)), key=lambda j: norm(sub(p, vo[j][0]))) for p, _, _ in vl]
                if perm != sorted(perm) or len(set(perm)) != len(perm):
                    if 'order' not in first and verbose:
                        print('t%02d n%d order %s %s\n  live %s\n  ours %s' % (i, n, kl, perm, [['%.3f' % x for x in p] + ['%.3f' % d] for p, _, d in vl], [['%.3f' % x for x in p] + ['%.3f' % d] for p, _, d in vo]))
                    first.setdefault('order', (n, kl, perm))
            elif len(vl) != len(vo):
                first.setdefault('count', (n, kl, len(vl), len(vo)))
    res[i] = first
    print('t%02d %s' % (i, first))
