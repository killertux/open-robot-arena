import sys,os; sys.path.insert(0,'.')
import pickle,math,bisect,statistics as st,collections
import rp_replay as R, rp_cmd2 as C
from rp_str import strengths
from rp_rod import live_series
rows=[]
for i in range(4):
    steps=pickle.load(open('live2s_%d.pkl'%i,'rb')); E=pickle.load(open('live2_%d.pkl'%i,'rb')); Sg=strengths(E['F'])
    T=[s[0] for s in steps]; us=live_series(E); UT=[x[0] for x in us]
    for n,ts in enumerate(R.starts(steps,E)):
        stk=[(t,d) for t,d in Sg if ts+0.1<=t<ts+0.5 and d>=95]
        bf=[t for t in E['BF'] if ts<=t<ts+0.1]
        if not stk or bf: continue
        tf=stk[0][0]
        byt=collections.defaultdict(dict)
        for l in open('rr/v_base_%d_%d.out'%(i,n)):
            f=l.split('\t'); byt[round(float(f[0]),3)][int(f[1])]=[float(x) for x in f[2:]]
        ots=sorted(byt)
        s0=steps[min(bisect.bisect_left(T,ts),len(steps)-1)]
        d=[s0[2][0]-s0[1][0],s0[2][2]-s0[1][2]]; dd=math.hypot(*d); u=(d[0]/dd,d[1]/dd)
        def L(tau,steps=steps,T=T,tf=tf,u=u):
            s=steps[min(bisect.bisect_left(T,tf+tau),len(steps)-1)]
            return ((s[1][0])*u[0]+(s[1][2])*u[1],(s[2][0])*u[0]+(s[2][2])*u[1],s[1][3]*u[0]+s[1][5]*u[1],s[2][3]*u[0]+s[2][5]*u[1])
        def O(tau,byt=byt,ots=ots,tf=tf,ts=ts,u=u):
            o=byt[min(ots,key=lambda x:abs(x-(tf-ts+tau)))]
            return ((o[0][0])*u[0]+(o[0][2])*u[1],(o[1][0])*u[0]+(o[1][2])*u[1],o[0][3]*u[0]+o[0][5]*u[1],o[1][3]*u[0]+o[1][5]*u[1])
        rows.append((L,O))
print(len(rows))
taus=[-0.2,-0.1,0,0.1,0.2,0.3,0.4,0.6,0.8,1.0,1.2]
for who,idx in (('Sentinel pos',0),('WL pos',1),('Sentinel vel',2),('WL vel',3)):
    for name,k in (('live',0),('ours',1)):
        vals=[]
        for tau in taus:
            v=[ (r[k](tau)[idx]-(r[k](0)[idx] if idx<2 else 0)) for r in rows]
            vals.append(round(st.median(v),2))
        print('%-13s %-4s'%(who,name),vals)
print('taus',taus)
dist=[]
for name,k in (('live',0),('ours',1)):
    print('gap WL-S rel to 0',name,[round(st.median([ (r[k](tau)[1]-r[k](tau)[0])-(r[k](0)[1]-r[k](0)[0]) for r in rows]),2) for tau in taus])
