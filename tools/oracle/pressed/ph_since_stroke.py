"""ph_since_stroke.py live <robotpair jsonl>... -- ours <filtered dump (G|B|R|A|F rows)>...: the Sentinel's place in the Wide Load's frame while
the robots are in contact, by the time since the Wide Load's last Poker stroke (docs/29 12ba).
Columns: time bin, share of pressed entries, median z, y (frame), median height difference (Sentinel minus WL), median centre distance."""
import sys, json, bisect, statistics as st, collections
mode = None; files = {'live': [], 'ours': []}
for a in sys.argv[1:]:
    if a in ('live', 'ours'): mode = a
    elif a != '--': files[mode].append(a)
def col(rot, k): return rot[3 * k:3 * k + 3]
def rel(wl_com, wl_rot, p):
    d = [p[i] - wl_com[i] for i in range(3)]
    return [sum(col(wl_rot, k)[i] * d[i] for i in range(3)) for k in range(3)]
BINS = [(0, 0.25), (0.25, 0.5), (0.5, 1.0), (1.0, 2.0), (2.0, 4.0), (4.0, 99)]
def summarize(name, data):
    print('==', name, 'pressed entries', len(data))
    for lo, hi in BINS:
        D = [d for d in data if lo <= d[0] < hi]
        if not D: continue
        print('  %4.2f-%4.1f s: %4.1f%%  z %.3f  y %.3f  x %.3f  h %+.3f  dist %.3f' % (lo, hi, 100 * len(D) / len(data), st.median(d[1] for d in D), st.median(d[2] for d in D), st.median(d[3] for d in D), st.median(d[4] for d in D), st.median(d[5] for d in D)))
data = []
for fn in files['live']:
    S = {}; C = set(); BF = []; clock = {}; t = 0.0; G = {}
    for l in open(fn):
        try: r = json.loads(l)
        except Exception: continue
        if r[0] == 'A': t += r[2]; clock[r[1]] = t
        elif r[0] == 'G': G[r[2]] = r[3]
        elif r[0] == 'S': S[r[1]] = r[2]
        elif r[0] == 'C': C.add(r[1])
        elif r[0] == 'BF': BF.append(r[1])
    bft = sorted(clock[s] for s in BF if s in clock)
    for q in sorted(C):
        s = S.get(q)
        if not s or len(s) < 2 or q not in clock: continue
        wl, se = (s[0], s[1]) if abs(G.get(s[0][0], 0) - 226.67) < 1 else (s[1], s[0])
        if wl[14] < 0.9 or se[14] < 0.9: continue
        i = bisect.bisect_right(bft, clock[q]) - 1
        since = clock[q] - bft[i] if i >= 0 else 99
        z = rel(wl[1:4], wl[10:19], se[1:4])
        data.append((since, z[2], z[1], z[0], se[2] - wl[2], ((se[1] - wl[1]) ** 2 + (se[3] - wl[3]) ** 2) ** .5))
summarize('live', data)
data = []
for fn in files['ours']:
    t = 0.0; B = {}; pressed = False; strokes = []; pend = []; lastneg = -9
    rows = []
    rob = lambda i: 0 if 5 <= i <= 10 else (1 if 11 <= i <= 16 else -1)
    for l in open(fn):
        r = json.loads(l); k = r[0]
        if k == 'A': t += r[2]
        elif k == 'F' and r[3] == 5 and r[4] < -500 and r[2].startswith('Wide'):
            if not strokes or t - strokes[-1] > 0.3 or t - lastneg > 0.1: strokes.append(t)
            lastneg = t
        elif k == 'R':
            pressed = any(rob(int(c[0][1:])) >= 0 and rob(int(c[1][1:])) >= 0 and rob(int(c[0][1:])) != rob(int(c[1][1:])) for c in r[3]); B = {}
        elif k == 'B' and pressed and r[2] in ('h5', 'h11'):
            B[r[2]] = r
            if len(B) == 2:
                se, wl = B['h5'], B['h11']
                if se[7][4] >= 0.9 and wl[7][4] >= 0.9:
                    i = bisect.bisect_right(strokes, t) - 1
                    since = t - strokes[i] if i >= 0 else 99
                    z = rel(wl[4], wl[7], se[4])
                    data.append((since, z[2], z[1], z[0], se[4][1] - wl[4][1], ((se[4][0] - wl[4][0]) ** 2 + (se[4][2] - wl[4][2]) ** 2) ** .5))
                B = {}
summarize('ours', data)
