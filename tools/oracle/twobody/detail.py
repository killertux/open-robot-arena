# detail.py <live.jsonl> <oursdir> <trial> [from] [to]: per step, live then ours: fork |w rel|, robot-robot contacts
# (body pair, point in ALARM's chassis frame, normal, distance) and the contact impulse (m dv, B row to V row) of both chassis and the fork
import sys, os, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *
meta, who, trials = load_live(sys.argv[1])
i = int(sys.argv[3]); a = int(sys.argv[4]) if len(sys.argv) > 4 else 0; b = int(sys.argv[5]) if len(sys.argv) > 5 else 40
names = {(0, 0): 'Sch'}
for k, bb in enumerate(meta['scout'][1:], 1): names[(0, k)] = 'Sfork' if bb[1] == 26 else 'Swh%d' % k
for k, bb in enumerate(meta['alarm'][1:], 1): names[(1, k)] = 'Arod' if bb[1] == 33 else 'Awh%d' % k
names[(1, 0)] = 'Ach'
L = per_step(trials[i]['rows'], who)
ow = {}
for l in open(os.path.join(sys.argv[2], 't%02d.state.out' % i)):
    p = l.split()
    if p and p[0] == 'MAP' and p[2] != '-':
        ow[p[3]] = (int(p[1]), int(p[2]))
orows = [json.loads(l) for l in open(os.path.join(sys.argv[2], 't%02d.state.dump' % i))]
O = per_step(orows, ow)
fork = [k for k, v in names.items() if v == 'Sfork'][0]
def show(tag, s):
    if not s or (0, 0) not in s['B']: print('   ', tag, '-'); return
    B, V = s['B'], s['V']
    wf = norm(sub(B[fork][6], B[(0, 0)][6]))
    A = B[(1, 0)]; RT = rot_T(A[7])
    out = []
    for c in s['R'] or []:
        ka, kb = who_of(c[0]), who_of(c[1])
        if ka is None or kb is None or ka[0] == kb[0]: continue
        if ka[0] == 1: ka, kb = kb, ka; n = [-x for x in c[2]]
        else: n = c[2]
        pl = RT(sub(c[3], A[4])); nl = RT(n)
        out.append('%s-%s p(%5.2f %5.2f %5.2f) n(%5.2f %5.2f %5.2f) d %.3f' % (names[ka], names[kb], *pl, *nl, c[4]))
    imp = []
    for k in ((0, 0), (1, 0), fork):
        if k in V:
            dv = sub(V[k][3], B[k][5]); m = B[k][3]
            imp.append('%s J %5.1f %5.1f %5.1f' % (names[k], *[m * x for x in dv]))
    print('   %s fork %5.1f pieces %d | %s | %d rr: %s' % (tag, wf, s['pieces'], '; '.join(imp), len(out), ' || '.join(out)))
def who_of(h):
    return who.get(h) if h in who else ow.get(h)
for n in range(a, b):
    print(n)
    show('L', L.get(n)); show('O', O.get(n))
