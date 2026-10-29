# stepcmp.py <live.jsonl> <dir> [trials]: examples/stepprobe.rs's predictions (O rows, tNN.sp) against the
# live B rows of the next substep, per body: centre, velocity and angular velocity errors, and their bias
# along the live motion. V=<body> lists one body step by step (docs/29 section 12ae).
import sys, os, json, math, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *
args = [a for a in sys.argv[1:] if not a.startswith('-')]
meta, who, trials = load_live(args[0]); D = args[1]
sel = [int(x) for x in args[2].split(',')] if len(args) > 2 else sorted(trials)
verbose = os.environ.get('V')
nm = {(0, 0): 'Sch', (1, 0): 'Ach'}
for k, bb in enumerate(meta['scout'][1:], 1): nm[(0, k)] = 'Sfork' if bb[1] == 26 else 'Swh%d' % k
for k, bb in enumerate(meta['alarm'][1:], 1): nm[(1, k)] = 'Arod' if bb[1] == 33 else 'Awh%d' % k
ptr2 = {}
for i, b in enumerate(meta['scout']): ptr2[b[0]] = nm[(0, i)]
for i, b in enumerate(meta['alarm']): ptr2[b[0]] = nm[(1, i)]
st = collections.defaultdict(list)
for i in sel:
    fn = os.path.join(D, 't%02d.sp' % i)
    if not os.path.exists(fn): continue
    L = {}; n = None; first = False; inB = False; prev = {}
    for r in trials[i]['rows']:
        if r[0] == 'T': n = r[2]; first = True
        elif r[0] == 'R': inB = first; first = False
        elif r[0] == 'B' and inB and n is not None: L.setdefault(n, {})[r[2]] = r
        elif r[0] != 'B': inB = False
    for l in open(fn):
        p = l.split()
        if p[0] != 'O': continue
        s = int(p[1]); ptr = p[2]; o = list(map(float, p[3:]))
        if s not in L or ptr not in L[s] or s - 1 not in L or ptr not in L[s - 1]: continue
        b = L[s][ptr]; b0 = L[s - 1][ptr]
        ec = norm(sub(o[0:3], b[4])); ev = norm(sub(o[3:6], b[5])); ew = norm(sub(o[6:9], b[6]))
        sv = norm(sub(b[5], b0[5])); sw = norm(sub(b[6], b0[6]))
        st[ptr2[ptr]].append((ec, ev, ew, sv, sw, i, s, o, b))
        if verbose and ptr2[ptr] == verbose:
            print('%2d %3d c %.4f v %.3f w %.3f | live dw %.3f | w live %s ours %s' % (i, s, ec, ev, ew, sw, ' '.join('%6.2f' % x for x in b[6]), ' '.join('%6.2f' % x for x in o[6:9])))
def pc(x, q): x = sorted(x); return x[min(len(x) - 1, int(q * len(x)))] if x else float('nan')
print('body   n     | err c p50 p90 | err v p50 p90 | err w p50 p90 | live step |dv| p50, |dw| p50')
for k in sorted(st):
    x = st[k]
    print('%-6s %5d | %.4f %.4f | %.3f %.3f | %.3f %.3f | %.3f %.3f' % (k, len(x), pc([a[0] for a in x], .5), pc([a[0] for a in x], .9),
          pc([a[1] for a in x], .5), pc([a[1] for a in x], .9), pc([a[2] for a in x], .5), pc([a[2] for a in x], .9), pc([a[3] for a in x], .5), pc([a[4] for a in x], .5)))
# bias: per body, mean of (ours - live) . unit(live w) for w, and . unit(live v) for v, over samples with |w| > 0.3
print('bias along live w / v direction (|.| > 0.3): mean, mean |err|, n')
for k in sorted(st):
    bw = []; bv = []
    for a in st[k]:
        o, b = a[7], a[8]
        wl = b[6]; nw = norm(wl)
        if nw > 0.3: bw.append(sum((o[6 + j] - wl[j]) * wl[j] / nw for j in range(3)))
        vl = b[5]; nv = norm(vl)
        if nv > 0.3: bv.append(sum((o[3 + j] - vl[j]) * vl[j] / nv for j in range(3)))
    if bw and bv:
        print('%-6s w %+.4f (n %d)  v %+.4f (n %d)' % (k, sum(bw) / len(bw), len(bw), sum(bv) / len(bv), len(bv)))
