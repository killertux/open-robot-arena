# resolveours.py <live.jsonl> <orderprobe.out> <orderprobe.dump> [dumpk=<file>]: per live resolver entry, the
# resolver (resolver_core.py) from the live B states fed the live contacts and ours (examples/orderprobe.rs with
# every body at its live pose, FRAME lines given); every body's error against the live V rows (docs/29 12ae).
import sys, json, math, collections
sys.path.insert(0, __import__('os').path.dirname(__file__))
from la import *
import resolver_core as rc
live, out, dump = sys.argv[1:4]
VAR = dict(a.split('=') for a in sys.argv[4:])
h2p = {}
for l in open(out):
    p = l.split()
    if p and p[0] == 'MAP': h2p[p[4]] = p[1]
O = {}; cur = None
for l in open(dump):
    r = json.loads(l)
    if r[0] == 'T': cur = r[2]
    elif r[0] == 'R' and cur is not None: O[cur] = r[3]; cur = None
P = {}; k = -1; ent = None; res = []
def done(ent):
    if ent is None or ent['k'] not in O or not ent['V']: return
    bodies = rc.bodies_from(ent['B'], P)
    lc = [rc.live_contact(c) for c in ent['R'][3]]
    oc = [dict(a=h2p.get(c[0], 'W' + c[0]), b=h2p.get(c[1], 'W' + c[1]), n=list(c[2]), p=list(c[3]), d=c[4], mu=c[5], e=c[7]) for c in O[ent['k']]]
    bl = rc.copy(bodies); rc.resolve(bl, lc)
    bo = rc.copy(bodies); rc.resolve(bo, oc)
    for p, v in ent['V'].items():
        if p not in bodies: continue
        res.append((ent['k'], p, norm(sub(bl[p]['v'], v[3])) + norm(sub(bl[p]['w'], v[4])),
                    norm(sub(bo[p]['v'], v[3])), norm(sub(bo[p]['w'], v[4])),
                    norm(sub(v[3], ent['B'][p][5])), norm(sub(v[4], ent['B'][p][6])), len(lc), len(oc)))
for l in open(live):
    r = json.loads(l); t = r[0]
    if t == 'P': rc.learn(P, r)
    elif t == 'R':
        done(ent); k += 1; ent = dict(k=k, R=r, B={}, V={})
    elif t == 'B' and ent is not None and r[1] == ent['R'][1]: ent['B'][r[2]] = r
    elif t == 'V' and ent is not None and r[1] == ent['R'][1]: ent['V'][r[2]] = r
done(ent)
names = {}
for l in open(out):
    p = l.split()
    if p and p[0] == 'MAP': names[p[1]] = 'r%sp%s' % (p[2], p[3])
by = collections.defaultdict(list)
for x in res: by[names.get(x[1], x[1])].append(x)
print('body: n moved | live-contacts err p99 | ours-contacts: dv err p50/p90, dw err p50/p90 | live |dv| p50 |dw| p50')
for b in sorted(by):
    xs = [x for x in by[b] if x[5] + x[6] > 1e-6]
    if not xs: continue
    print('%-6s %5d | %.1e | %.3f %.3f  %.3f %.3f | %.3f %.3f' % (b, len(xs), rc.pct([x[2] for x in xs], 99),
        rc.pct([x[3] for x in xs], 50), rc.pct([x[3] for x in xs], 90), rc.pct([x[4] for x in xs], 50), rc.pct([x[4] for x in xs], 90),
        rc.pct([x[5] for x in xs], 50), rc.pct([x[6] for x in xs], 50)))
if VAR.get('dumpk'):
    with open(VAR['dumpk'], 'w') as f:
        for x in res: f.write('%d %s %s\n' % (x[0], names.get(x[1], x[1]), ' '.join('%.5f' % y for y in x[2:])))
