#!/usr/bin/env python3
"""rp_reduce2.py live|ours <in> <out.pkl>: {'contact':[t...], 'W':[(t,robot,[(thrust,touch,n)...])], 'N':[(t,robot,n)], 'F':[(t,kind)], 'chassis':[(t,S,WL)]}
robot 0 = Sentinel, 1 = Wide Load"""
import json,sys,pickle
kind,inp,outp=sys.argv[1:4]
D={'rod':[],'contact':[],'W':[],'N':[],'BF':[],'FM':[],'U':[],'F':[],'L':[]}
t=0.0
if kind=='live':
    rows=[json.loads(l) for l in open(inp) if l.strip()]
    G=[r[2] for r in rows if r[0]=='G']   # [WL, Sentinel]
    side={G[0]:1,G[1]:0}
    wheelside={}
    for r in rows:
        k=r[0]
        if k=='A': t+=r[2]
        elif k=='C': D['contact'].append(t)
        elif k=='W': D['W'].append((t,side.get(r[2],-1),[(w[1],w[2],tuple(w[3])) for w in r[3]]))
        elif k=='N': D['N'].append((t,r[2],tuple(r[3]),r[5],r[6]))
        elif k=='BF': D['BF'].append(t)
        elif k=='FM': D['FM'].append((t,r[2]))
        elif k=='U': D['U'].append((t,r[2],r[4],r[9],r[10],r[12]))
        elif k=='F': D['F'].append((t,r[3]))
        elif k=='L': D['L'].append((t,r[3]))
else:
    body_robot=lambda i: 0 if 5<=i<=10 else (1 if 11<=i<=16 else -1)
    rt=None
    f=open(inp)
    for l in f:
        if not l.startswith('["'): continue
        r=json.loads(l)
        k=r[0]
        if k=='A': t+=r[2]
        elif k=='RT':
            for c in r[2]:
                if body_robot(c[0])>=0 and body_robot(c[1])>=0 and body_robot(c[0])!=body_robot(c[1]):
                    D['contact'].append(t); break
        elif k=='G':
            b=int(r[2][1:]); D['W'].append((t,body_robot(b),[(w[0],w[1],tuple(w[2])) for w in r[3]]))
        elif k=='N': D['N'].append((t,None,tuple(r[4]),r[2],r[3]))
        elif k=='B' and r[2] in ('h11','h16'): D['rod'].append((t,r[2],tuple(r[4]),tuple(r[5])))
        elif k=='F' and r[2]=='Wide Load': D['F'].append((t,r[4]))
pickle.dump(D,open(outp,'wb'))
print(inp,{k:len(v) for k,v in D.items()},round(t,1))
