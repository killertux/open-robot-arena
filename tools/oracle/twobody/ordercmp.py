# ordercmp.py: the contacts our detection gives with every body at its live pose
# (examples/orderprobe.rs), against the live resolver's list (docs/29 section 12u).
#   ordercmp.py poses <live.jsonl> <poses.txt> [from] [to]   the orderprobe input (R rows by ordinal)
#   ordercmp.py cmp <live.jsonl> <ours.dump> <ours.out> [-v N] [-t] summary; -v: the first N mismatches
#   of each kind in full; -t: per pair type
# Live rows: every `R` row (one per resolver entry) followed by its `B` rows.
import sys, json, math
from collections import Counter, defaultdict

def frames(path, lo=0, hi=10**9):
    out = []  # (ordinal, R contacts, [B rows])
    k = -1
    for l in open(path):
        r = json.loads(l)
        if r[0] == 'R':
            k += 1
            if lo <= k < hi:
                out.append([k, r[3], []])
        elif r[0] == 'B' and out and out[-1][0] == k:
            out[-1][2].append(r)
    return out

REST = __import__('os').path.join(__import__('os').path.dirname(__import__('os').path.abspath(__file__)), '../../../local/oracle/original/twobody-scout-alarm-box__live__000.jsonl')
def poses(live, dst, lo=0, hi=10**9):
    with open(dst, 'w') as f:
        for l in open(REST):
            r = json.loads(l)
            if r[0] == 'REST':
                for c, q in r[1]: f.write('REST %s %s\n' % (' '.join(map(repr, c)), ' '.join(map(repr, q))))
        for k, R, B in frames(live, lo, hi):
            f.write('F %d\n' % k)
            for b in B:
                f.write('B %s %r %s %s %s %s\n' % (b[2], b[3], ' '.join(map(repr, b[4])), ' '.join(map(repr, b[7])),
                                                    ' '.join(map(repr, b[5])), ' '.join(map(repr, b[6]))))

def sub(a, b): return [x - y for x, y in zip(a, b)]
def norm(a): return math.sqrt(sum(x * x for x in a))

def cmp(live, dump, out, verbose=0, bytype=False):
    lmap = {}; omap = {}
    for l in open(out):
        p = l.split()
        if p and p[0] == 'MAP':
            lmap[p[1]] = omap[p[4]] = 'r%sp%s' % (p[2], p[3])
    L = {k: R for k, R, _ in frames(live)}
    O = {}; cur = None
    for l in open(dump):
        r = json.loads(l)
        if r[0] == 'T': cur = r[2]
        elif r[0] == 'R' and cur is not None: O[cur] = r[3]; cur = None
    def lab(m, h): return m.get(h, 'W')
    def groups(R, m):
        g = []
        for c in R:
            key = (lab(m, c[0]), lab(m, c[1]))
            if g and g[-1][0] == key: g[-1][1].append(c)
            else: g.append((key, [c]))
        return g
    stats = Counter(); shown = Counter(); kinds = defaultdict(Counter)
    def show(kind, k, txt):
        if shown[kind] < verbose:
            shown[kind] += 1
            print('--- %s at R %d\n%s' % (kind, k, txt))
    def fmt(g):
        return ' '.join('%s-%s:%d' % (str(a), str(b), len(v)) for (a, b), v in g)
    for k in sorted(O):
        if k not in L: continue
        gl, go = groups(L[k], lmap), groups(O[k], omap)
        stats['frames'] += 1
        cl = [tuple(sorted(map(str, key))) for key, _ in gl]; co = [tuple(sorted(map(str, key))) for key, _ in go]
        if sorted(cl) != sorted(co):
            stats['pair set differs'] += 1
            show('set', k, '  live %s\n  ours %s' % (fmt(gl), fmt(go)))
            continue
        if cl != co:
            stats['pair order differs'] += 1
            show('order', k, '  live %s\n  ours %s' % (fmt(gl), fmt(go)))
        else:
            stats['pair order same'] += 1
        od = {tuple(sorted(map(str, key))): (key, v) for key, v in go}
        for key, vl in gl:
            ck = tuple(sorted(map(str, key)))
            ko, vo = od[ck]
            ty = 'W' if 'W' in ck else ('rr' if ck[0][1] != ck[1][1] else 'self')
            ty = ty + ':' + '-'.join(str(x) for x in ck)
            kinds[ty]['pairs'] += 1
            if ko != key:
                stats['orientation differs'] += 1; kinds[ty]['orient'] += 1
                show('orient', k, '  live %s  ours %s' % (key, ko))
            if len(vl) != len(vo):
                stats['point count differs'] += 1; kinds[ty]['count'] += 1
                continue
            if len(vl) > 1:
                perm = [min(range(len(vo)), key=lambda j: norm(sub(c[3], vo[j][3]))) for c in vl]
                far = max(norm(sub(c[3], vo[j][3])) for c, j in zip(vl, perm))
                if len(set(perm)) != len(perm) or far > 0.01:
                    stats['points differ (>1 cm)'] += 1; kinds[ty]['far'] += 1
                elif perm != sorted(perm):
                    stats['point order differs'] += 1; kinds[ty]['porder'] += 1
                    show('porder', k, '  %s %s\n  live %s\n  ours %s' % (key, perm,
                         ['(%.3f %.3f %.3f) %.4f' % (*c[3], c[4]) for c in vl], ['(%.3f %.3f %.3f) %.4f' % (*c[3], c[4]) for c in vo]))
                else:
                    stats['point order same'] += 1; kinds[ty]['psame'] += 1
    for s, n in stats.items(): print('%-24s %d' % (s, n))
    if bytype:
        for t in sorted(kinds, key=lambda t: -kinds[t]['pairs']):
            print('  %-28s %s' % (t, dict(kinds[t])))

if __name__ == '__main__':
    a = sys.argv[1:]
    if a[0] == 'poses':
        poses(a[1], a[2], *(int(x) for x in a[3:5]))
    else:
        v = int(a[a.index('-v') + 1]) if '-v' in a else 0
        cmp(a[1], a[2], a[3], v, '-t' in a)
