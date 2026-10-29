# pairstats.py <live.jsonl> <orderprobe.out> <orderprobe.dump>: per pair of body kinds, the points live and
# ours (orderprobe at the live poses), and for each live point the nearest of ours within 3 cm: the distance
# difference and the angle between the normals (docs/29 section 12ae).
import sys, json, math, collections
live, out, dump = sys.argv[1:4]
h2n = {}; p2n = {}
for l in open(out):
    p = l.split()
    if p and p[0] == 'MAP': h2n[p[4]] = p2n[p[1]] = 'r%sp%s' % (p[2], p[3])
def nm(x):
    return {'r0p0': 'Sch', 'r0p3': 'Sfork', 'r1p0': 'Ach', 'r1p5': 'Arod'}.get(x, ('Swh' if x.startswith('r0') else 'Awh') if x != 'W' else 'W')
L = {}; k = -1
for l in open(live):
    r = json.loads(l)
    if r[0] == 'R': k += 1; L[k] = r[3]
O = {}; cur = None
for l in open(dump):
    r = json.loads(l)
    if r[0] == 'T': cur = r[2]
    elif r[0] == 'R' and cur is not None: O[cur] = r[3]; cur = None
def key(c, m):
    a, b = nm(m.get(c[0], 'W')), nm(m.get(c[1], 'W'))
    n = c[2]
    if (a, b) > (b, a): a, b = b, a; n = [-x for x in n]
    return (a, b), n
cnt = collections.defaultdict(lambda: [0, 0]); dd = collections.defaultdict(list); ang = collections.defaultdict(list)
for k in O:
    if k not in L: continue
    lg = collections.defaultdict(list); og = collections.defaultdict(list)
    for c in L[k]:
        kk, n = key(c, p2n); lg[kk].append((c[3], n, c[4]))
    for c in O[k]:
        kk, n = key(c, h2n); og[kk].append((c[3], n, c[4]))
    for kk in set(lg) | set(og):
        cnt[kk][0] += len(lg[kk]); cnt[kk][1] += len(og[kk])
        for p, n, d in lg[kk]:
            best = min(og[kk], key=lambda o: math.dist(o[0], p), default=None)
            if best and math.dist(best[0], p) < 0.03:
                dd[kk].append(best[2] - d)
                ang[kk].append(math.degrees(math.acos(max(-1, min(1, sum(x * y for x, y in zip(n, best[1])))))))
def med(x): x = sorted(x); return x[len(x) // 2] if x else float('nan')
for kk in sorted(cnt, key=lambda q: -cnt[q][0]):
    print('%-14s live %6d ours %6d | matched %6d dd mean %+.4f med %+.4f | angle med %.1f p90 %.1f' % (
        '-'.join(kk), cnt[kk][0], cnt[kk][1], len(dd[kk]), sum(dd[kk]) / max(1, len(dd[kk])), med(dd[kk]), med(ang[kk]),
        sorted(ang[kk])[int(0.9 * len(ang[kk]))] if ang[kk] else float('nan')))
