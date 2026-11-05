"""ph_touch.py live <robotpair jsonl>... -- ours <RA2_H17_DUMP jsonl>...: the share of resolver entries at which each wheel of the
Compressor's robots has its `touching` flag set (the MagicAction's wheel records, `W` live / `G` ours), upright robots only
(chassis up.y > 0.7), free and pressed (a contact between the robots at that entry) (docs/29 12ba)."""
import sys, json, collections
mode = None; files = {'live': [], 'ours': []}
for a in sys.argv[1:]:
    if a in ('live', 'ours'): mode = a
    elif a != '--': files[mode].append(a)
rob = lambda i: 0 if 5 <= i <= 10 else (1 if 11 <= i <= 16 else -1)
def report(name, T):
    print('==', name)
    for k in sorted(T):
        s, n = T[k]
        print('  robot %d %-7s wheel %d  touching %.3f  (n %d)' % (k[0], k[1], k[2], s / n, n))
T = collections.defaultdict(lambda: [0, 0])
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
    # the engaged window of the fight: from the first entry with the robots within 8 m after 5 s, for 60 s (rp_w.py)
    t0 = None
    for q in sorted(S):
        s_ = S[q]
        if len(s_) >= 2 and clock.get(q, 0) > 5 and ((s_[0][1] - s_[1][1]) ** 2 + (s_[0][3] - s_[1][3]) ** 2) ** .5 < 8:
            t0 = clock[q]; break
    if t0 is None: continue
    for r in W:
        if not (t0 <= clock.get(r[1], -1) <= t0 + 60): continue
        m = G.get(r[2], 0)
        who = 0 if abs(m - 296.39) < 1 else (1 if abs(m - 226.67) < 1 else None)
        s = S.get(r[1])
        if who is None or not s or len(s) < 2: continue
        up = {0: s[1][14], 1: s[0][14]}  # S rows: [0] is the 226.67 kg chassis (Wide Load), [1] the Sentinel
        if up[0] < 0.7 or up[1] < 0.7: continue
        pr = 'pressed' if r[1] in C else 'free'
        for i, w in enumerate(r[3]):
            T[(who, pr, i)][0] += w[2]; T[(who, pr, i)][1] += 1
report('live', T)
T = collections.defaultdict(lambda: [0, 0])
for fn in files['ours']:
    up = {}; pressed = False; seq = None; pend = []
    for l in open(fn):
        if not l.startswith('["'): continue
        r = json.loads(l); k = r[0]
        if k == 'R':
            pressed = any(rob(int(c[0][1:])) >= 0 and rob(int(c[1][1:])) >= 0 and rob(int(c[0][1:])) != rob(int(c[1][1:])) for c in r[3])
            up = {}; pend = []
        elif k == 'B' and r[2] in ('h5', 'h11'):
            up[0 if r[2] == 'h5' else 1] = r[7][4]
        elif k == 'G' and r[2] in ('h5', 'h11'):
            pend.append(r)
            if len(pend) and len(up) == 2:
                pass
        if k == 'G' and r[2] in ('h5', 'h11') and len(up) == 2:
            who = 0 if r[2] == 'h5' else 1
            if up[0] >= 0.7 and up[1] >= 0.7:
                pr = 'pressed' if pressed else 'free'
                for i, w in enumerate(r[3]):
                    T[(who, pr, i)][0] += w[1]; T[(who, pr, i)][1] += 1
report('ours', T)
