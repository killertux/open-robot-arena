# cmp.py <live.jsonl> <oursdir> [trial]: outcome table (both chassis), or one trial step by step
import sys, os, json, math
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *
meta, who, trials = load_live(sys.argv[1])
D = sys.argv[2]
def ours_steps(i):
    """from the .out S lines: steps[n][robot] = (c, up, w, v)"""
    st = {}
    for l in open(os.path.join(D, 't%02d.state.out' % i)):
        p = l.split()
        if p and p[0] == 'S':
            n, r = int(p[1]), int(p[2]); f = list(map(float, [x for x in p[3:] if x not in ('c', 'up', 'w', 'v')]))
            st.setdefault(n, {})[r] = (f[0:3], f[3:6], f[6:9], f[9:12])
    return st
def live_steps(i):
    st = {}
    for n, s in per_step(trials[i]['rows'], who).items():
        if n < 0: continue
        for r in (0, 1):
            b = s['B'].get((r, 0))
            if b:
                rot = b[7]
                st.setdefault(n, {})[r] = (b[4], [rot[3], rot[4], rot[5]], b[6], b[5])
    return st
def summary(st):
    o = []
    for r in (0, 1):
        ups = [st[n][r][1][1] for n in sorted(st) if r in st[n]]
        ws = [norm(st[n][r][2]) for n in sorted(st) if r in st[n]]
        last = max(n for n in st if r in st[n])
        o.append('min up %5.2f end up %5.2f max|w| %4.1f end c %s' % (min(ups), ups[-1], max(ws), ' '.join('%6.2f' % x for x in st[last][r][0])))
    return o
if len(sys.argv) > 3:
    i = int(sys.argv[3]); L = live_steps(i); O = ours_steps(i)
    for n in sorted(L):
        if n not in O: continue
        s = '%3d' % n
        for r in (0, 1):
            l, o = L[n][r], O[n][r]
            s += ' | r%d dc %.3f up %5.2f/%5.2f w %5.2f %5.2f %5.2f / %5.2f %5.2f %5.2f' % (r, norm(sub(l[0], o[0])), l[1][1], o[1][1], *l[2], *o[2])
        print(s)
else:
    for i in sorted(trials):
        if not os.path.exists(os.path.join(D, 't%02d.state.out' % i)): continue
        L, O = summary(live_steps(i)), summary(ours_steps(i))
        print('%2d %-24s' % (i, trials[i]['spec'].get('label')))
        for r in (0, 1):
            print('   %s live %s\n   %s ours %s' % ('SA'[r], L[r], 'SA'[r], O[r]))
