# mkours.py <live.jsonl> <outdir> [trials]: an examples/twoprobe state file per
# trial of a capture_twobody.py capture (docs/29 section 12s): the live placed
# state, the throttles, every fire the live rows show (`M` fire settings of the
# fork's burst motor, `F` one-shot pushes of ALARM's piston, or the fork's onset
# when its `M` row was deduplicated away) and the live component ticks.
# SHIFT (default 1): live events after the T row of substep k go to our drive
# step k - SHIFT, which puts our fork's onset on live's (checked on all 20
# trials of twobody-scout-alarm-box__live__000).
import sys, os, json
sys.path.insert(0, os.path.dirname(__file__))
from common import *
meta, who, trials = load_live(sys.argv[1])
out = sys.argv[2]; os.makedirs(out, exist_ok=True)
sel = [int(x) for x in sys.argv[3].split(',')] if len(sys.argv) > 3 else sorted(trials)
masses = [b[1] for b in meta['scout']] + [b[1] for b in meta['alarm']]
robots = [0]*len(meta['scout']) + [1]*len(meta['alarm'])
SHIFT = int(os.environ.get('SHIFT', '1'))
for i in sel:
    tr = trials[i]; spec = tr['spec']
    place = [r for r in tr['rows'] if r[0] == 'X' and r[2] == 'place'][0]
    steps = per_step(tr['rows'], who)
    fires = []
    fork = 1 + [b[1] for b in meta['scout'][1:]].index(26)
    prev = 0
    for n in sorted(steps):
        if n < 0: continue
        for e in steps[n]['ev']:
            if e[0] == 'F' and e[3] == -1000:
                fires.append((1, n, 'push after T %d' % n))
            if e[0] == 'M' and e[3] < -20 and e[4] > 1500:
                fires.append((0, n, 'motor fire after T %d' % n))
        B = steps[n]['B']
        if (0, 0) in B and (0, fork) in B:
            w = norm(sub(B[(0, fork)][6], B[(0, 0)][6]))
            if w > 12 and prev < 3:
                fires.append((0, n - 2, 'fork onset at B %d (%.1f)' % (n, w)))
            prev = w
    with open(os.path.join(out, 't%02d.state' % i), 'w') as f:
        f.write('# %s\n' % spec.get('label'))
        for (p, c, q, v, w), m, rb in zip(place[3], masses, robots):
            f.write(' '.join(map(str, [rb, m] + c + q + v + w)) + '\n')
        th = spec.get('throttle', [0, 0])
        f.write('throttle 0 %s\nthrottle 1 %s\n' % (th[0], th[1]))
        seen = set()
        motor = [s for rb, s, why in fires if why.startswith('motor')]
        fires = [x for x in fires if not (x[2].startswith('fork') and any(abs(x[1] - m) <= 2 for m in motor))]
        for rb, s, why in fires:
            f.write('# fire %s\n' % why)
            if (rb, s) not in seen and s - SHIFT >= 0:
                seen.add((rb, s))
                f.write('fire %d %d\n' % (rb, s - SHIFT))
        n = None
        for r in tr['rows']:
            if r[0] == 'T': n = r[2]
            elif r[0] == 'TM' and n is not None and n - SHIFT >= 0: f.write('tick %d %s\n' % (n - SHIFT, r[2]))
    print(i, spec.get('label'), fires)
