# mkposes.py <live.jsonl> <frames.txt> <outdir>: per trial tNN.poses for examples/stepprobe.rs: the FRAME
# lines (spincmp.py frames) and the B rows of each substep's first resolver entry (docs/29 section 12ae).
import sys, os, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *
meta, who, trials = load_live(sys.argv[1])
fr = open(sys.argv[2]).read()
os.makedirs(sys.argv[3], exist_ok=True)
for i, tr in trials.items():
    with open(os.path.join(sys.argv[3], 't%02d.poses' % i), 'w') as f:
        f.write(fr)
        n = None; first = False; inB = False
        for r in tr['rows']:
            if r[0] == 'T': n = r[2]; first = True; inB = False
            elif r[0] == 'R':
                inB = first and n is not None and n >= 0
                if inB: f.write('F %d\n' % n)
                first = False
            elif r[0] == 'B' and inB:
                f.write('B %s %r %s %s %s %s\n' % (r[2], r[3], ' '.join(map(repr, r[4])), ' '.join(map(repr, r[7])), ' '.join(map(repr, r[5])), ' '.join(map(repr, r[6]))))
            elif r[0] != 'B': inB = False
