import sys; sys.path.insert(0,'.')
import pickle,math,bisect,statistics as st,collections
from rp_str import strengths
def live_ser(E):
    out=[]
    for t,c,dr,lk,al,b in E['U']:
        if not b: continue
        ch=[x for x in b if abs(x[2]-226.67)<1]; rd=[x for x in b if abs(x[2]-63)<1]
        if ch and rd: out.append((t,ch[0][3],ch[0][4],rd[0][3],rd[0][4]))
    return out
def ours_ser(E):
    ch={};out=[]
    for t,name,com,vel in E['rod']:
        if name=='h11': ch[t]=(com,vel)
        else:
            c=ch.get(t)
            if c: out.append((t,c[0],c[1],com,vel))
    return out
def axis(ser):
    # nose axis: from chassis com to rod com, averaged
    v=[(r[3][i]-r[1][i]) for r in ser[:200] for i in range(3)]
    return None
def run(name,ser,fires,pressed,strong=95):
    T=[x[0] for x in ser]
    res=collections.defaultdict(list)
    for tf,dr in fires:
        if dr<strong: continue
        k0=bisect.bisect_left(T,tf)
        if k0+40>=len(ser): continue
        a=ser[min(k0+8,len(ser)-1)]   # axis from rod extended position
        ax=[a[3][i]-a[1][i] for i in range(3)]; n=math.sqrt(sum(x*x for x in ax)); ax=[x/n for x in ax]
        def vch(tau):
            k=min(bisect.bisect_left(T,tf+tau),len(ser)-1); r=ser[k]; return sum(r[2][i]*ax[i] for i in range(3))
        def vrd(tau):
            k=min(bisect.bisect_left(T,tf+tau),len(ser)-1); r=ser[k]; return sum(r[4][i]*ax[i] for i in range(3))
        key='pressed' if pressed(tf) else 'free'
        res[key].append((vch(0.0),vch(0.1),vch(0.16),vch(0.24),vch(0.3),vch(0.4),vch(0.5),vch(0.7),max(vrd(0.28),vrd(0.3),vrd(0.32))))
    for key,R in res.items():
        print(name,key,len(R),'chassis v along nose axis at tau 0,.1,.16,.24,.3,.4,.5,.7:',[round(st.median(r[k] for r in R),2) for k in range(8)],'| rod v max near return %.1f'%st.median(r[8] for r in R))
if __name__=='__main__':
    for i in range(4):
        pass
    L=[];O=[]
    ltot=collections.defaultdict(list)
    for name,rng in (('live',range(4)),('ours',range(16))):
        agg=[]
        for i in rng:
            E=pickle.load(open(('live2_%d.pkl' if name=='live' else 'ours/base/e%d.pkl')%i,'rb'))
            ser=live_ser(E) if name=='live' else ours_ser(E)
            S=strengths(E['F']); C=E['contact']
            pr=lambda t,C=C:(lambda k:k<len(C) and C[k]<=t+0.2)(bisect.bisect_left(C,t-0.2))
            run(name+str(i) if False else name,ser,S,pr)
