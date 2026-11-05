import sys; sys.path.insert(0,'.')
import pickle,math,bisect,statistics as st,collections
import rp_cmd2 as C
def strengths(F):
    res=[]
    for i in range(len(F)-1):
        t,f=F[i]
        if f>-1500 and f<0 and F[i+1][1]<-1500 and F[i+1][0]-t<0.05: res.append((t,-F[i+1][1]/100))
    return res
def go(name,items):
    R=[];W=[]
    for steps,E in items:
        T=[s[0] for s in steps]
        t0=None
        for s in steps:
            if math.dist([s[1][0],s[1][2]],[s[2][0],s[2][2]])<8 and s[0]>5: t0=s[0];break
        if t0 is None: continue
        def at(t):
            k=min(bisect.bisect_left(T,t),len(steps)-1); return steps[k]
        for ts,dr in strengths(E['F']):
            if not(t0<=ts<=t0+60): continue
            W.append(dr)
            s0=at(ts); d0=math.hypot(s0[1][0]-s0[2][0],s0[1][2]-s0[2][2])
            if d0>2.8 or s0[1][13]<0.9 or s0[2][13]<0.9: continue
            R.append((dr,max(math.hypot(at(ts+0.1*k)[1][0]-at(ts+0.1*k)[2][0],at(ts+0.1*k)[1][2]-at(ts+0.1*k)[2][2]) for k in range(16)),math.hypot(at(ts+0.5)[1][0]-at(ts+0.5)[2][0],at(ts+0.5)[1][2]-at(ts+0.5)[2][2])-d0))
    print(name,'strokes in window',len(W),'mean dir %.1f'%st.mean(W),'share dir<40: %.2f'%(sum(w<40 for w in W)/len(W)),'dir<30: %.2f'%(sum(w<30 for w in W)/len(W)))
    for lo,hi in ((0,30),(30,60),(60,90),(90,101)):
        sel=[r for r in R if lo<=r[0]<hi]
        if len(sel)>=8: print('   dir %d-%d n=%d max d within 1.5 s median %.2f share>3.2 %.2f ; dd@0.5 median %.2f'%(lo,hi,len(sel),st.median(r[1] for r in sel),sum(r[1]>3.2 for r in sel)/len(sel),st.median(r[2] for r in sel)))
if __name__=="__main__":
  live=[(pickle.load(open('live2s_%d.pkl'%i,'rb')),pickle.load(open('live2_%d.pkl'%i,'rb'))) for i in range(4)]
  ours=[(pickle.load(open('ours/base/s%d.pkl'%i,'rb')),pickle.load(open('ours/base/e%d.pkl'%i,'rb'))) for i in range(16)]
  go("live",live);go("ours",ours)
