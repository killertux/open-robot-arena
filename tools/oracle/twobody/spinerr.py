# spinerr.py <live.jsonl> <dir>: stepprobe's one-step error of every wheel's spin about its axle (relative
# to its chassis), by robot throttle and by whether the wheel touched anything (docs/29 section 12ae).
import sys, os, json, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *
meta, who, trials = load_live(sys.argv[1]); D = sys.argv[2]
res = collections.defaultdict(list)
for i in sorted(trials):
    fn = os.path.join(D, 't%02d.sp' % i)
    if not os.path.exists(fn): continue
    th = trials[i]['spec'].get('throttle', [0, 0])
    L = {}; n = None; first = False; inB = False; touched = collections.defaultdict(set)
    for r in trials[i]['rows']:
        if r[0] == 'T': n = r[2]; first = True
        elif r[0] == 'R':
            for c in r[3]: touched[n] |= {c[0], c[1]}
            inB = first; first = False
        elif r[0] == 'B' and inB and n is not None: L.setdefault(n, {})[r[2]] = r
        elif r[0] != 'B': inB = False
    O = collections.defaultdict(dict)
    for l in open(fn):
        p = l.split()
        if p[0] == 'O': O[int(p[1])][p[2]] = list(map(float, p[3:]))
    for robot, ri in (('scout', 0), ('alarm', 1)):
        ch = meta[robot][0][0]
        for k, b in enumerate(meta[robot]):
            if b[1] != 27: continue
            w = b[0]
            for s in O:
                if s not in L or s - 1 not in L or w not in O[s]: continue
                R = L[s][ch][7]; ax = [R[0], R[1], R[2]]
                lv = sum(x * y for x, y in zip(sub(L[s][w][6], L[s][ch][6]), ax))
                ov = sum(x * y for x, y in zip(sub(O[s][w][6:9], O[s][ch][6:9]), ax))
                pv = sum(x * y for x, y in zip(sub(L[s - 1][w][6], L[s - 1][ch][6]), ax))
                key = (robot[0] + str(k), 'th%d' % th[ri], 'touch' if w in touched[s - 1] else 'air')
                sgn = 1 if pv >= 0 else -1
                res[key].append(((ov - lv) * sgn, abs(ov - lv)))
for k in sorted(res):
    x = res[k]
    if len(x) < 20: continue
    print('%-4s %-6s %-5s n %4d err along spin mean %+.3f | mean |err| %.3f' % (*k, len(x), sum(a for a, b in x) / len(x), sum(b for a, b in x) / len(x)))
