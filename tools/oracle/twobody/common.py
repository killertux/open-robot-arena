import json, math
def load_live(path):
    meta = json.load(open(path[:-1]))
    rows = [json.loads(l) for l in open(path)]
    ptrs = [b[0] for b in meta['scout']] + [b[0] for b in meta['alarm']]
    who = {p: (0, i) for i, p in enumerate(b[0] for b in meta['scout'])}
    who.update({p: (1, i) for i, p in enumerate(b[0] for b in meta['alarm'])})
    trials = {}; cur = None; step = None
    for r in rows:
        if r[0] == 'TRIAL':
            cur = trials[r[1]] = dict(spec=r[2], rows=[]); step = None
        elif cur is not None:
            cur['rows'].append(r)
    return meta, who, trials

def per_step(rows, who):
    """steps[n] = dict(B={(robot,i): row}, V={...}, R=[contacts of the first piece], pieces=k)"""
    steps = {}; n = None; cur = None; inR = False
    for r in rows:
        t = r[0]
        if t == 'T':
            n = r[2]; cur = steps.setdefault(n, dict(B={}, V={}, R=None, pieces=0, Rall=[], ev=[]))
        elif cur is None:
            continue
        elif t == 'R':
            cur['pieces'] += 1
            cur['Rall'].append(r[3])
            if cur['R'] is None: cur['R'] = r[3]; inR = True
            else: inR = False
        elif t == 'B' and inR:
            k = who.get(r[2])
            if k: cur['B'][k] = r
        elif t == 'V' and inR:
            k = who.get(r[2])
            if k: cur['V'][k] = r
        elif t in ('M', 'F', 'FIREPY'):
            cur['ev'].append(r)
    return steps

def rot_cols(r):  # 9 floats by columns -> function applying R
    def ap(v):
        return [r[0]*v[0]+r[3]*v[1]+r[6]*v[2], r[1]*v[0]+r[4]*v[1]+r[7]*v[2], r[2]*v[0]+r[5]*v[1]+r[8]*v[2]]
    return ap
def rot_T(r):
    def ap(v):
        return [r[0]*v[0]+r[1]*v[1]+r[2]*v[2], r[3]*v[0]+r[4]*v[1]+r[5]*v[2], r[6]*v[0]+r[7]*v[1]+r[8]*v[2]]
    return ap
def sub(a, b): return [x-y for x, y in zip(a, b)]
def norm(a): return math.sqrt(sum(x*x for x in a))
