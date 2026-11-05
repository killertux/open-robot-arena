"""ph_replay.py <tag> [ENV=VAL ...]: the 79 live pressed starts of the Compressor (docs/29 12ay) replayed in ours
(examples/pairreplay) and the arrangement compared with live's at 0.25 .. 2.0 s (docs/29 12ba).

PRESSED_WORK holds live2s_<i>.pkl (rp_reduce.py of the live captures) and rr/r<i>_<n>.state/.script (rp_replay.py);
PRESSED_GAME is a scratch install; PH_OUT where this writes (default $PRESSED_WORK/ph/<tag>).
Prints, per quantity, the median over starts of (ours - live) and the share of starts where ours is the larger."""
import sys, os, math, pickle, glob, subprocess, statistics as st, collections, bisect
W = os.environ['PRESSED_WORK']; G = os.environ['PRESSED_GAME']
BIN = os.environ.get('PH_BIN', os.path.expanduser('~/Projects/ora-wt/pressheight/engine/target/release/examples/pairreplay'))
tag = sys.argv[1]
env = dict(os.environ)
for a in sys.argv[2:]:
    k, v = a.split('='); env[k] = v
O = os.environ.get('PH_OUT', f'{W}/ph') + '/' + tag
os.makedirs(O, exist_ok=True)
TAUS = (0.0, 0.25, 0.5, 1.0, 1.5, 2.0)

def frame(q):
    r = q[9:18]  # columns x y z of the chassis
    return [r[0:3], r[3:6], r[6:9]]
def rel(a, b):
    """b's centre in a's frame (x, y, z)."""
    cols = frame(a)
    d = [b[i] - a[i] for i in range(3)]
    return [sum(c[i] * d[i] for i in range(3)) for c in cols]

def live_at(steps, T, t):
    k = min(bisect.bisect_left(T, t), len(steps) - 1); return steps[k]

rows = collections.defaultdict(list)
for i in range(4):
    steps = pickle.load(open(f'{W}/live2s_{i}.pkl', 'rb')); T = [s[0] for s in steps]
    for sf in sorted(glob.glob(f'{W}/rr/r{i}_*.state')):
        n = sf.split('_')[-1].split('.')[0]
        lines = open(sf).read().strip().split('\n')
        c0 = [float(x) for x in lines[0].split()[1:4]]
        k = min(range(len(steps)), key=lambda j: abs(steps[j][1][0] - c0[0]) + abs(steps[j][1][2] - c0[2]) + abs(steps[j][1][1] - c0[1]) if True else 0)
        # the nearest live step by the first robot's centre (the state came from there)
        best = None
        for j in range(max(0, k - 400), min(len(steps), k + 400)):
            d = sum(abs(steps[j][1][m] - c0[m]) for m in range(3))
            if best is None or d < best[0]: best = (d, j)
        ts = steps[best[1]][0]
        if os.environ.get('PH_CLEAN'):
            # only starts with no Flip in the 2 s and no Poker stroke in the 2.5 s before (the replay puts the
            # flipper, rods and wheels at rest)
            E = pickle.load(open(f'{W}/live2_{i}.pkl', 'rb'))
            if any(ts - 2.0 <= t <= ts for t, c in E['FM']) or any(ts - 2.5 <= t <= ts for t in E['BF']): continue
        tg = f'{O}/r{i}_{n}'
        for ext in ('state', 'script'):
            open(f'{tg}.{ext}', 'w').write(open(f'{W}/rr/r{i}_{n}.{ext}').read())
        mod = os.environ.get('PH_MOD')
        if mod:
            # PH_MOD: nofire (no Poker stroke), noflip (no Flip), nopress (neither), idleS / idleW (that robot drives nothing)
            out = []
            for l in open(tg + '.script').read().strip().split('\n'):
                f = l.split()
                if mod in ('nofire', 'nopress'): f[6] = '0'
                if mod in ('noflip', 'nopress'): f[5] = '0'
                if mod == 'idleS': f[1] = '0'; f[2] = '0'
                if mod == 'idleW': f[3] = '0'; f[4] = '0'
                out.append(' '.join(f))
            open(tg + '.script', 'w').write('\n'.join(out) + '\n')
        if not os.path.exists(tg + '.out') or os.environ.get('PH_RERUN'):
            if os.environ.get('PH_DUMP'): env['RA2_H17_DUMP'] = tg + '.dump'
            r = subprocess.run([BIN, G + '/Arenas/compressor.py', G + '/AI/Team0/Bot2.bot', G + '/AI/Team8/Bot2.bot',
                                tg + '.state', tg + '.script', '2.5', tg + '.out'], capture_output=True, text=True, cwd=G, env=env)
            if r.returncode != 0: print(r.stderr[-300:]); continue
        byt = collections.defaultdict(dict)
        for l in open(tg + '.out'):
            f = l.split('\t'); byt[round(float(f[0]), 3)][int(f[1])] = [float(x) for x in f[2:]]
        ots = sorted(byt)
        for tau in TAUS:
            ls = live_at(steps, T, ts + tau)
            b = byt[min(ots, key=lambda x: abs(x - tau))]
            if 0 not in b or 1 not in b: continue
            # ours: cx cy cz vx vy vz wx wy wz rot(9)
            oq = {r_: b[r_][0:3] + b[r_][3:6] + b[r_][6:9] + b[r_][9:18] for r_ in (0, 1)}
            lq = {0: list(ls[1]), 1: list(ls[2])}
            # S = Sentinel (robot 0), W = Wide Load (robot 1)
            lr = rel(lq[1], lq[0]); orr = rel(oq[1], oq[0])
            def vfr(q, v): return sum(frame(lq[1])[2][i] * v[i] for i in range(3))
            lw = [vfr(None, lq[1][3:6]), vfr(None, lq[0][3:6])]
            ow = [sum(frame(lq[1])[2][i] * oq[1][3 + i] for i in range(3)), sum(frame(lq[1])[2][i] * oq[0][3 + i] for i in range(3))]
            rows[tau].append(dict(dvw=ow[0] - lw[0], dvs=ow[1] - lw[1], dvy_s=oq[0][4] - lq[0][4], dvy_w=oq[1][4] - lq[1][4], i=i, n=n, lx=lr[0], ly=lr[1], lz=lr[2], ox=orr[0], oy=orr[1], oz=orr[2],
                                  lh=lq[0][1] - lq[1][1], oh=oq[0][1] - oq[1][1],
                                  ld=math.hypot(lq[0][0]-lq[1][0], lq[0][2]-lq[1][2]), od=math.hypot(oq[0][0]-oq[1][0], oq[0][2]-oq[1][2])))
print(tag)
for tau in TAUS:
    R = rows[tau]
    if not R: continue
    def m(k): return st.median(r['o' + k] - r['l' + k] for r in R)
    def p(k, q): return sorted(r['o' + k] - r['l' + k] for r in R)[int(q * (len(R) - 1))]
    print('t %.2f n %d | WL frame, ours-live median: x %+.3f y %+.3f z %+.3f | height diff %+.3f (p25 %+.3f p75 %+.3f) | dist %+.3f | live z med %.3f ours %.3f | live h med %+.3f ours %+.3f' % (
        tau, len(R), m('x'), m('y'), m('z'), m('h'), p('h', .25), p('h', .75), m('d'),
        st.median(r['lz'] for r in R), st.median(r['oz'] for r in R), st.median(r['lh'] for r in R), st.median(r['oh'] for r in R)))

    print('      velocity ours-live along WL z: WL %+.3f Sentinel %+.3f | vertical: WL %+.3f Sentinel %+.3f' % (st.median(r['dvw'] for r in R), st.median(r['dvs'] for r in R), st.median(r['dvy_w'] for r in R), st.median(r['dvy_s'] for r in R)))
