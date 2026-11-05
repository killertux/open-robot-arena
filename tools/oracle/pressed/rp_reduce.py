#!/usr/bin/env python3
"""rp_reduce.py live|ours <in> <out.pkl>  -> list of steps (t, sentinel(com,vel,ang,rot), wl(...), contacts[(tagS,tagW,n(from S),p,dist)])"""
import json,sys,pickle
kind,inp,outp=sys.argv[1:4]
steps=[]
t=0.0
if kind=='live':
    rows=[json.loads(l) for l in open(inp) if l.strip()]
    G=[r[2] for r in rows if r[0]=='G']   # [WL, Sentinel]
    # robot id -> side: decided from contact points later; assume learn order WL, Sentinel
    cur={}
    pend=None
    for r in rows:
        k=r[0]
        if k=='A': t+=r[2]
        elif k=='C': pend=r[2]
        elif k=='S':
            d={x[0]:x for x in r[2]}
            if len(d)<2: pend=None; continue
            wl=d[G[0]]; se=d[G[1]]
            cs=[]
            if pend:
                for x in pend:
                    cs.append((x[0],x[1],x[2],x[3],tuple(x[4:7]),tuple(x[7:10]),x[10]))
            steps.append((t,tuple(se[1:]),tuple(wl[1:]),cs)); pend=None
else:
    # ours: R,RT,B... per resolver call
    body_robot=lambda i: 0 if 5<=i<=10 else (1 if 11<=i<=16 else -1)
    pr=None; prt=None; B={}
    def flush():
        global pr,prt,B
        if pr is None: return
        if 'h5' in B and 'h11' in B:
            cs=[]
            for c,tg in zip(pr,prt):
                a=int(c[0][1:]); b=int(c[1][1:])
                ra,rb=body_robot(a),body_robot(b)
                if ra<0 or rb<0 or ra==rb: continue
                cs.append((tg[2],tg[3],ra,rb,tuple(c[2]),tuple(c[3]),c[4]))
            se=B['h5']; wl=B['h11']
            steps.append((t,tuple(se[0]+se[1]+se[2]+se[3]),tuple(wl[0]+wl[1]+wl[2]+wl[3]),cs))
        pr=prt=None; B={}
    f=sys.stdin if inp=='-' else open(inp)
    for l in f:
        if not l.startswith('["'): continue
        r=json.loads(l)
        k=r[0]
        if k=='R': flush(); pr=r[3]
        elif k=='RT': prt=r[2]
        elif k=='B': B[r[2]]=(r[4],r[5],r[6],r[7])
        elif k=='A': t+=r[2]
    flush()
pickle.dump(steps,open(outp,'wb'))
print(inp,len(steps),round(t,1))
