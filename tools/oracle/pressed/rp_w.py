import sys; sys.path.insert(0,'.')
import pickle,glob,bisect,math,io,contextlib,collections,statistics as st
import rp_an2 as A
def window(steps):
    t0=None
    for s in steps:
        if A.norm(A.sub(s[1][0:3],s[2][0:3]))<8 and s[0]>5: t0=s[0]; break
    return t0
def run(name,pairs):
    agg=collections.defaultdict(lambda: collections.defaultdict(list))
    ev=collections.defaultdict(int); secs=0.0
    dur=collections.Counter()
    for sp,ep in pairs:
        steps=pickle.load(open(sp,'rb')); E=pickle.load(open(ep,'rb'))
        t0=window(steps)
        if t0 is None: continue
        T=[s[0] for s in steps]
        C=E['contact']
        def pressed(t):
            i=bisect.bisect_left(C,t-0.2); return i<len(C) and C[i]<=t+0.2
        for t,r,ws in E['W']:
            if not(t0<=t<=t0+60) or r<0: continue
            i=min(bisect.bisect_left(T,t),len(steps)-1)
            s=steps[i]
            if s[1][13]<0.7 or s[2][13]<0.7: continue
            pr=pressed(t); key=(r,'pressed' if pr else 'free')
            dur[key]+=1
            tw=[w for w in ws if w[1]]
            agg[key]['ntouch'].append(len(tw)); agg[key]['nwheels'].append(len(ws))
            agg[key]["thrust"].append(sum(abs(w[0] or 0) for w in tw))
            agg[key]['ny'].append(sum(1 for w in tw if w[2][1]<0.7)/max(1,len(tw)))
            agg[key]['nymean'].append(st.mean(w[2][1] for w in tw) if tw else 1)
        secs+=60
        for e in E['N']:
            t=e[0]
            if not(t0<=t<=t0+60): continue
            ny=e[2][1]
            robot=e[3] if name=='ours' else {2:0,1:1}.get(e[4],'x')   # live: other robot 2 (WL) -> wheel is Sentinel's
            ev[(robot if robot is not None else 'x',('pressed' if pressed(t) else 'free'), 'floor' if ny>0.9 else ('nonfloor'))]+=1
    print(name)
    for key in sorted(agg):
        a=agg[key]
        print('  ',key,dur[key],{k:round(st.mean(v),3) for k,v in a.items()})
    print('  N events per min of window',{k:round(v/secs*60,1) for k,v in sorted(ev.items(),key=str)})
live=[('live2s_%d.pkl'%i,'live2_%d.pkl'%i) for i in range(4)]
import sys
TAG=sys.argv[1] if len(sys.argv)>1 else 'base'
ours=[('ours/%s/s%d.pkl'%(TAG,i),'ours/%s/e%d.pkl'%(TAG,i)) for i in range(16)]
run('live',live); run('ours',ours)
