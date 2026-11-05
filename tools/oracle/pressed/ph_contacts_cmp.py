"""ph_contacts_cmp.py live <robotpair jsonl>... -- ours <ph_contacts_ours.py tsv>...: the contacts between the Compressor's robots in
the Wide Load's frame, live against ours (docs/29 12ba). Per contact: the point about the WL's centre of mass (x right, y up, z forward
in its frame), the normal there, the Sentinel's up.y."""
import sys, json, math, statistics as st, collections
def col(rot, k): return rot[3 * k:3 * k + 3]
def tof(com, rot, p):
    d = [p[i] - com[i] for i in range(3)]
    return [sum(col(rot, k)[i] * d[i] for i in range(3)) for k in range(3)]
def tov(rot, n): return [sum(col(rot, k)[i] * n[i] for i in range(3)) for k in range(3)]
L, O = [], []; mode = None; files = {'live': [], 'ours': []}
for a in sys.argv[1:]:
    if a in ('live', 'ours'): mode = a
    elif a != '--': files[mode].append(a)
entries = {'live': 0, 'ours': 0}
def add(kind, wl, se, p, n, d):
    # pressed contacts of upright robots only
    if wl[10 + 4] < 0.9 or se[10 + 4] < 0.9: return
    pw = tof(wl[1:4], wl[10:19], p); nw = tov(wl[10:19], n); ps = tof(se[1:4], se[10:19], p); ns = tov(se[10:19], n)
    (L if kind == 'live' else O).append((pw, nw, ps, ns, d, wl[1:4], se[1:4]))
for fn in files['live']:
    S = {}; rows = []
    for l in open(fn):
        try: r = json.loads(l)
        except Exception: continue
        if r[0] == 'S': S[r[1]] = r[2]
        elif r[0] == 'C': rows.append(r)
    for r in rows:
        s = S.get(r[1])
        if not s or len(s) < 2: continue
        entries['live'] += 1
        for c in r[2]:
            add('live', s[0], s[1], c[7:10], c[4:7], c[10])
for fn in files['ours']:
    last = None
    for l in open(fn):
        head, a, b = l.split('|')
        f = head.split(); t = float(f[0])
        if t != last: entries['ours'] += 1; last = t
        se = [0] + [float(x) for x in a.split()[:3]] + [0] * 6 + [float(x) for x in a.split()[3:12]]
        wl = [0] + [float(x) for x in b.split()[:3]] + [0] * 6 + [float(x) for x in b.split()[3:12]]
        add('ours', wl, se, [float(x) for x in f[6:9]], [float(x) for x in f[3:6]], float(f[9]))
print('entries (with a contact)', entries, 'contacts', len(L), len(O))
for nm, D in (('live', L), ('ours', O)):
    n = entries[nm]
    print('==', nm, 'contacts per entry %.2f' % (len(D) / n))
    for k, name in ((0, 'point in WL frame (x,y,z) median'), (1, 'normal in WL frame median'), (2, 'point in Sentinel frame median'), (3, 'normal in Sentinel frame')):
        print('  ', name, [round(st.median(d[k][i] for d in D), 3) for i in range(3)], 'p25', [round(sorted(d[k][i] for d in D)[len(D) // 4], 3) for i in range(3)], 'p75', [round(sorted(d[k][i] for d in D)[3 * len(D) // 4], 3) for i in range(3)])
    print('   dist median %.3f' % st.median(d[4] for d in D))
    h = collections.Counter()
    for d in D:
        h[(round(d[0][2] * 2) / 2, round(d[0][1] * 2) / 2)] += 1
    print('   per 100 entries, WL-frame (z,y) cells:', {k: round(100 * v / n, 1) for k, v in sorted(h.items(), key=lambda x: -x[1])[:14]})
