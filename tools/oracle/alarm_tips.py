#!/usr/bin/env python3
"""alarm_tips.py live <boxstart-*.jsonl>... | ours <ensemble dir with f<w>.txt, t<w>.tsv> (docs/29 12ap).

ALARM's tips in the Box against the Scout's fork strokes and its own hammer
strokes, with one definition for the original and for ours:
  tip      ALARM's up.y falls through 0.5 after it was above 0.8;
  to back  its up.y is below -0.5 at some time in the next 3 s;
  fork     the Scout's burst motor starts a stroke (live: hinge motor set with
           velocity <= -40 and torque >= 3000, the `M` rows of
           frida/ra2_boxweapons.js; ours: FIDTRACE_FIRE, robot 0 hinge, firing);
  hammer   ALARM's piston is released (live: the `F` row of -1000 on the
           controller that also pushes +-10000; ours: robot 1 slider, firing).
Time is the fight clock (live: from the first chassis row; ours: the match clock). The 5th value of
each sample is the Scout's up.y.
Prints per-run lines and the totals: tips per 100 s, tips per fork stroke that
ends with the Scout near ALARM, the times of the fork and hammer strokes
before each tip.
"""
import ast, glob, json, math, os, sys


def live(path):
    t = 0.0; g = []; ups = []; fork = []; ham = []; t0 = None
    piston = set(); shots = []
    pos = {}
    for l in open(path):
        r = json.loads(l)
        k = r[0]
        if k == 'A':
            t += r[2]
        elif k == 'G':
            g.append(r[2])
        elif k == 'X':
            if t0 is None:
                t0 = t
            d = {c[0]: c for c in r[2]}
            a, s = d[g[1]], d[g[0]]
            ups.append((t - t0, a[2][4], a[1], s[1], s[2][4]))
        elif k == 'M' and t0 is not None and r[3] <= -40 and r[4] >= 3000:
            fork.append(t - t0)
        elif k == 'F' and t0 is not None:
            # ALARM's piston is the controller that pushes +-10000 (the arena's
            # sliders push 2000 and 40000); its release is the one-shot -1000 times the air level.
            if abs(r[3]) == 10000:
                piston.add(r[2])
            elif -1000.5 <= r[3] < 0:
                shots.append((t - t0, r[2]))
    ham = [x for x, c in shots if c in piston]
    return ups, fork, ham


def ours(fp):
    d = os.path.dirname(fp); w = os.path.basename(fp)[1:-4]
    ups = []
    for l in open(f'{d}/t{w}.tsv').read().splitlines()[1:]:
        a, b = l.split('\t', 1)
        rows = ast.literal_eval(b)
        ups.append((float(a), rows[1][5], rows[1][1], rows[0][1], rows[0][5]))
    fork, ham = [], []
    for l in open(fp):
        p = l.split()
        if p[1] == 'Z':
            continue
        if p[4] == '1':
            if p[1] == '0' and p[3] == '0':
                fork.append(float(p[0]))
            elif p[1] == '1' and p[3] == '1':
                ham.append(float(p[0]))
    return ups, fork, ham


def tips(ups, fork, ham, tmax):
    out = []; was = True
    for k in range(1, len(ups)):
        t, u = ups[k][0], ups[k][1]
        if t > tmax:
            break
        if was and u < 0.5:
            was = False
            back = min(x[1] for x in ups[k:] if x[0] <= t + 3) < -0.5
            out.append((t, back, [round(x - t, 2) for x in fork if t - 1.5 <= x <= t + 0.3],
                        [round(x - t, 2) for x in ham if t - 1.0 <= x <= t + 0.3]))
        elif u > 0.8:
            was = True
    return out


def main():
    mode, files = sys.argv[1], sys.argv[2:]
    if mode == 'ours' and len(files) == 1 and os.path.isdir(files[0]):
        files = sorted(glob.glob(files[0] + '/f*.txt'), key=lambda p: int(os.path.basename(p)[1:-4]))
    T = n = nb = 0; nf = nforkprev = 0; forks = 0
    for fp in files:
        ups, fork, ham = (live if mode == 'live' else ours)(fp)
        if not ups:
            continue
        # the fight ends when the clock stops moving or 120 s
        tmax = min(120.0, ups[-1][0])
        # live: stop at the last sample before the Scout (player 0) goes idle (kept simple: 120 s)
        ev = tips(ups, fork, ham, tmax)
        T += tmax; n += len(ev); nb += sum(e[1] for e in ev)
        forks += len([x for x in fork if x <= tmax])
        for e in ev:
            if e[2]:
                nforkprev += 1
        print(f'{os.path.basename(fp)} {tmax:.0f} s: forks {len([x for x in fork if x <= tmax])}, hammers {len([x for x in ham if x <= tmax])}, '
              + '; '.join(f'tip {e[0]:.1f}{" BACK" if e[1] else ""} fork{e[2]} ham{e[3]}' for e in ev))
    print(f'{mode}: {len(files)} runs, {T:.0f} s, tips {n} ({100*n/T:.2f} per 100 s), to back {nb} ({100*nb/T:.2f}), '
          f'tips with a fork stroke in the 1.5 s before {nforkprev}, fork strokes {forks} ({forks/T:.3f} /s)')


main()
