# resolvecmp.py <live.jsonl> [maxentries] [worst=N]: replay the resolver (docs/29 section 2, resolver.rs) on
# every live resolver entry of a capture_twobody.py capture (R contacts, B states, each body's live inertia from
# its P row) and compare every body's velocities with the live V rows (docs/29 section 12ae).
import sys, json, math
sys.path.insert(0, __import__('os').path.dirname(__file__))
from la import *
path = sys.argv[1]
MAXE = int(sys.argv[2]) if len(sys.argv) > 2 else 10**9
VAR = dict(a.split('=') for a in sys.argv[3:])
TOL = 0.1
P = {}

def resolve(bodies, contacts):
    def pv(b, p):
        if b not in bodies: return zeros(3)
        B = bodies[b]; return add(B['v'], cross(B['w'], sub(p, B['com'])))
    def K(b, p):
        if b not in bodies: return [[0.0] * 3 for _ in range(3)]
        B = bodies[b]; r = skew(sub(p, B['com']))
        return msub(eye(1 / B['m']), mm(mm(r, B['Iinv']), r))
    def apply(b, j, p):
        if b not in bodies: return
        B = bodies[b]; B['v'] = add(B['v'], mul(j, 1 / B['m'])); B['w'] = add(B['w'], mv(B['Iinv'], cross(sub(p, B['com']), j)))
    parent = {}
    def find(x):
        parent.setdefault(x, x)
        while parent[x] != x: parent[x] = parent[parent[x]]; x = parent[x]
        return x
    for c in contacts:
        if c['a'] in bodies and c['b'] in bodies:
            ra, rb = find(c['a']), find(c['b'])
            if ra != rb: parent[ra] = rb
    groups = {}; order = []
    for i, c in enumerate(contacts):
        own = c['a'] if c['a'] in bodies else c['b']
        if own not in bodies: continue
        r = find(own)
        if r not in groups: groups[r] = []; order.append(r)
        groups[r].append(i)
    nimp = 0
    for g in order:
        isl = groups[g]; resp = []
        for i in isl:
            c = contacts[i]; k = madd(K(c['a'], c['p']), K(c['b'], c['p']))
            iv = inv(k) if abs(det(k)) > 1e-12 else [[0.0] * 3 for _ in range(3)]
            resp.append((iv, dot(c['n'], mv(k, c['n']))))
        cnt = len(isl); cur = 0; spent = 0
        while spent < 400:
            found = None
            for _ in range(cnt):
                k = cur; cur = (cur + 1) % cnt
                c = contacts[isl[k]]
                u = sub(pv(c['a'], c['p']), pv(c['b'], c['p']))
                if dot(c['n'], u) < -0.001 and resp[k][1] > 1e-12:
                    found = (k, u); break
            if found is None: break
            k, u = found; c = contacts[isl[k]]; iv, kn = resp[k]
            e = 0.0 if spent > 100 else c['e']; mu = c['mu']; n = c['n']
            a = -dot(n, u) / kn; p0 = mul(n, a); w = mv(iv, mul(u, -1)); d = sub(w, p0); pp = add(mul(p0, 1 + e), d)
            pn = dot(pp, n); pt = norm(sub(pp, mul(n, pn)))
            if mu * pn < pt:
                wt = norm(sub(w, mul(n, dot(w, n)))); s = (1 + e) * a * mu / (wt - dot(d, n) * mu)
                j = add(mul(p0, 1 + e), mul(d, s))
            else:
                j = pp
            apply(c['a'], j, c['p']); apply(c['b'], mul(j, -1), c['p'])
            edge = TOL * 0.9; f = (edge - c['d']) / edge
            if 0 < f < 1 and VAR.get('push', '1') == '1':
                u2 = sub(pv(c['a'], c['p']), pv(c['b'], c['p']))
                if dot(n, u2) > 0:
                    dv = mul(n, TOL * 10 * f * f)
                    for b, sg in ((c['a'], 1), (c['b'], -1)):
                        if b in bodies: apply(b, mul(dv, bodies[b]['m'] * sg), c['p'])
            spent += 1; nimp += 1
    return nimp

stats = []; ne = 0; cur = None
def flush(cur):
    global ne
    if cur is None or not cur['V']: return
    R, B, V = cur['R'], cur['B'], cur['V']
    bodies = {}
    for p, b in B.items():
        if p not in P: continue
        m, Iloc = P[p]
        rc = b[7]; rot = [[rc[0], rc[3], rc[6]], [rc[1], rc[4], rc[7]], [rc[2], rc[5], rc[8]]]  # by columns
        bodies[p] = dict(m=m, Iinv=mm(mm(rot, inv(Iloc)), T(rot)), com=list(b[4]), v=list(b[5]), w=list(b[6]))
    contacts = [dict(a=c[0], b=c[1], n=list(c[2]), p=list(c[3]), d=c[4], mu=math.sqrt(c[5] * c[6]), e=c[7] * c[8]) for c in R[3]]
    resolve(bodies, contacts)
    for p, v in V.items():
        if p not in bodies: continue
        dv = norm(sub(v[3], B[p][5])); dw = norm(sub(v[4], B[p][6]))
        ev = norm(sub(bodies[p]['v'], v[3])); ew = norm(sub(bodies[p]['w'], v[4]))
        stats.append((R[1], p, dv, dw, ev, ew, len(R[3])))
    ne += 1
for l in open(path):
    r = json.loads(l); t = r[0]
    if t == 'P':
        f = r[3]; o = 0x19c // 4
        P[r[2]] = (f[0x190 // 4], [f[o:o + 3], f[o + 4:o + 7], f[o + 8:o + 11]])
    elif t == 'R':
        flush(cur); cur = dict(R=r, B={}, V={})
        if ne >= MAXE: break
    elif t == 'B' and cur is not None and r[1] == cur['R'][1]:
        cur['B'][r[2]] = r
    elif t == 'V' and cur is not None and r[1] == cur['R'][1]:
        cur['V'][r[2]] = r
flush(cur)
mov = [s for s in stats if s[2] + s[3] > 1e-6]
print('entries', ne, 'body samples', len(stats), 'moved', len(mov))
for name, col in (('err v', 4), ('err w', 5)):
    x = [s[col] for s in mov]
    print(name, 'p50 %.2e p90 %.2e p99 %.2e max %.2e' % tuple(pct(x, q) for q in (50, 90, 99, 100)))
rel = [(s[4] + s[5]) / (s[2] + s[3]) for s in mov]
print('rel err p50 %.3f p90 %.3f p99 %.3f; > 0.1: %d of %d' % (pct(rel, 50), pct(rel, 90), pct(rel, 99), sum(r > 0.1 for r in rel), len(rel)))
if VAR.get('worst'):
    for s, r in sorted(zip(mov, rel), key=lambda x: -x[1])[:int(VAR['worst'])]: print(s, round(r, 3))
