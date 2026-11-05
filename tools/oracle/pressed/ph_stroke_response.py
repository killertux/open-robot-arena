"""ph_stroke_response.py live <robotpair jsonl>... -- ours <filtered dump>...: the Sentinel's place and speed in the Wide Load's frame around each
Poker stroke that starts in contact (docs/29 12ba). Rows: time from the stroke's fire, median z, vz (relative speed), the Wide Load's speed
along its z, the Sentinel's, over the strokes."""
import sys, json, bisect, statistics as st
mode = None; files = {'live': [], 'ours': []}
for a in sys.argv[1:]:
    if a in ('live', 'ours'): mode = a
    elif a != '--': files[mode].append(a)
def col(rot, k): return rot[3 * k:3 * k + 3]
def fr(rot, v): return [sum(col(rot, k)[i] * v[i] for i in range(3)) for k in range(3)]
OFF = [-0.2, -0.1, 0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.8, 1.0]
def at(series, t):
    ts = [x[0] for x in series]; i = bisect.bisect_left(ts, t)
    return series[min(i, len(series) - 1)] if series else None
def run(name, trajs):
    print('==', name, 'strokes', len(trajs))
    for j, o in enumerate(OFF):
        v = [tr[j] for tr in trajs if tr[j]]
        if not v: continue
        print('  %+.1f s: z %.3f  dz/dt(rel) %+.2f  WL vz %+.2f  Sentinel vz %+.2f  h %+.3f' % (o, st.median(x[0] for x in v), st.median(x[1] for x in v), st.median(x[2] for x in v), st.median(x[3] for x in v), st.median(x[4] for x in v)))
trajs = []
for fn in files['live']:
    S = {}; clock = {}; t = 0.0; BF = []; G = {}; C = set()
    for l in open(fn):
        try: r = json.loads(l)
        except Exception: continue
        if r[0] == 'A': t += r[2]; clock[r[1]] = t
        elif r[0] == 'G': G[r[2]] = r[3]
        elif r[0] == 'S': S[r[1]] = r[2]
        elif r[0] == 'BF': BF.append(r[1])
        elif r[0] == 'C': C.add(r[1])
    ser = []
    for q in sorted(S):
        s = S[q]
        if len(s) < 2 or q not in clock: continue
        wl, se = (s[0], s[1]) if abs(G.get(s[0][0], 0) - 226.67) < 1 else (s[1], s[0])
        d = [se[1 + i] - wl[1 + i] for i in range(3)]
        z = fr(wl[10:19], d)[2]
        vr = fr(wl[10:19], [se[4 + i] - wl[4 + i] for i in range(3)])[2]
        ser.append((clock[q], z, vr, fr(wl[10:19], wl[4:7])[2], fr(wl[10:19], se[4:7])[2], se[2] - wl[2], q in C))
    for b in BF:
        if b not in clock: continue
        t0 = clock[b]
        j = at([(x[0], x) for x in ser], t0)
        if not j or not j[1][6]: continue
        # in contact at the fire and both upright (z near -2.3)
        tr = []
        for o in OFF:
            p = at([(x[0], x) for x in ser], t0 + o)
            tr.append((p[1][1], p[1][2], p[1][3], p[1][4], p[1][5]) if p else None)
        if tr[2] and -3 < tr[2][0] < -1.5: trajs.append(tr)
run('live', trajs)
trajs = []
rob = lambda i: 0 if 5 <= i <= 10 else (1 if 11 <= i <= 16 else -1)
for fn in files['ours']:
    t = 0.0; B = {}; ser = []; strokes = []; pressed = False; lastneg = -9
    for l in open(fn):
        r = json.loads(l); k = r[0]
        if k == 'A': t += r[2]
        elif k == 'F' and r[3] == 5 and r[4] < -500 and r[2].startswith('Wide'):
            if not strokes or t - strokes[-1] > 0.3 or t - lastneg > 0.1: strokes.append(t)
            lastneg = t
        elif k == 'R':
            pressed = any(rob(int(c[0][1:])) >= 0 and rob(int(c[1][1:])) >= 0 and rob(int(c[0][1:])) != rob(int(c[1][1:])) for c in r[3]); B = {}
        elif k == 'B' and r[2] in ('h5', 'h11'):
            B[r[2]] = r
            if len(B) == 2:
                se, wl = B['h5'], B['h11']
                d = [se[4][i] - wl[4][i] for i in range(3)]
                z = fr(wl[7], d)[2]
                vr = fr(wl[7], [se[5][i] - wl[5][i] for i in range(3)])[2]
                ser.append((t, z, vr, fr(wl[7], wl[5])[2], fr(wl[7], se[5])[2], se[4][1] - wl[4][1], pressed)); B = {}
    for t0 in strokes:
        p = at([(x[0], x) for x in ser], t0)
        if not p or not p[1][6]: continue
        tr = []
        for o in OFF:
            q = at([(x[0], x) for x in ser], t0 + o)
            tr.append((q[1][1], q[1][2], q[1][3], q[1][4], q[1][5]) if q else None)
        if tr[2] and -3 < tr[2][0] < -1.5: trajs.append(tr)
run('ours', trajs)
