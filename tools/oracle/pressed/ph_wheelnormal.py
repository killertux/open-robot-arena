"""ph_wheelnormal.py live <robotpair jsonl>... -- ours <filtered dump>...: the surface normal each touching wheel of the Compressor's robots carries in its
MagicAction record (the drive direction d = -(axle x n)), in the chassis frame, upright robots in the engaged window; free and pressed
(docs/29 12ba). Per robot and state: share of touching wheel records with n.y < 0.9, mean n (x y z), mean |thrust| sign aside."""
import sys, json, collections, statistics as st
mode = None; files = {'live': [], 'ours': []}
for a in sys.argv[1:]:
    if a in ('live', 'ours'): mode = a
    elif a != '--': files[mode].append(a)
rob = lambda i: 0 if 5 <= i <= 10 else (1 if 11 <= i <= 16 else -1)
def col(rot, k): return rot[3 * k:3 * k + 3]
def tl(rot, n): return [sum(col(rot, k)[i] * n[i] for i in range(3)) for k in range(3)]
def report(name, T):
    print('==', name)
    for key in sorted(T):
        v = T[key]
        print('  robot %d %-7s n %6d  tilted(n.y<0.9) %.3f  mean n (x,y,z) %+.3f %+.3f %+.3f' % (key[0], key[1], len(v), sum(1 for n in v if n[1] < 0.9) / len(v), *(st.mean(n[i] for n in v) for i in range(3))))
        tl_ = [n for n in v if n[1] < 0.9]
        if tl_:
            h = collections.Counter((round(n[1] * 4) / 4, round(n[2] * 4) / 4) for n in tl_)
            print('     tilted (n.y, n.z) cells, share of all:', {k: round(c / len(v), 3) for k, c in h.most_common(8)})
T = collections.defaultdict(list)
for fn in files['live']:
    G = {}; S = {}; C = set(); W = []; clock = {}; t = 0.0
    for l in open(fn):
        try: r = json.loads(l)
        except Exception: continue
        if r[0] == 'A': t += r[2]; clock[r[1]] = t
        if r[0] == 'G': G[r[2]] = r[3]
        elif r[0] == 'S': S[r[1]] = r[2]
        elif r[0] == 'C': C.add(r[1])
        elif r[0] == 'W': W.append(r)
    t0 = None
    for q in sorted(S):
        s_ = S[q]
        if len(s_) >= 2 and clock.get(q, 0) > 5 and ((s_[0][1] - s_[1][1]) ** 2 + (s_[0][3] - s_[1][3]) ** 2) ** .5 < 8: t0 = clock[q]; break
    if t0 is None: continue
    for r in W:
        if not (t0 <= clock.get(r[1], -1) <= t0 + 60): continue
        m = G.get(r[2], 0); who = 0 if abs(m - 296.39) < 1 else (1 if abs(m - 226.67) < 1 else None)
        s = S.get(r[1])
        if who is None or not s or len(s) < 2: continue
        wl, se = (s[0], s[1]) if abs(G.get(s[0][0], 0) - 226.67) < 1 else (s[1], s[0])
        if wl[14] < 0.7 or se[14] < 0.7: continue
        rot = se[10:19] if who == 0 else wl[10:19]
        pr = 'pressed' if r[1] in C else 'free'
        for w in r[3]:
            if w[2]: T[(who, pr)].append(tl(rot, w[3]))
report('live', T)
T = collections.defaultdict(list)
for fn in files['ours']:
    up = {}; rots = {}; pressed = False
    for l in open(fn):
        r = json.loads(l); k = r[0]
        if k == 'R':
            pressed = any(rob(int(c[0][1:])) >= 0 and rob(int(c[1][1:])) >= 0 and rob(int(c[0][1:])) != rob(int(c[1][1:])) for c in r[3]); rots = {}
        elif k == 'B' and r[2] in ('h5', 'h11'): rots[0 if r[2] == 'h5' else 1] = r[7]
        elif k == 'G' and r[2] in ('h5', 'h11') and len(rots) == 2:
            who = 0 if r[2] == 'h5' else 1
            if rots[0][4] < 0.7 or rots[1][4] < 0.7: continue
            for w in r[3]:
                if w[1]: T[(who, 'pressed' if pressed else 'free')].append(tl(rots[who], w[2]))
report('ours', T)
