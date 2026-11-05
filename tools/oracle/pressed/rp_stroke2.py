import sys; sys.path.insert(0,'.')
import pickle,math,bisect,statistics as st,collections
import rp_cmd2 as C
from rp_stroke import strokes_ours
def go(name,items):
    R=[]
    for steps,E,strokes,cm in items:
        T=[s[0] for s in steps]
        t0=None
        for s in steps:
            if math.dist([s[1][0],s[1][2]],[s[2][0],s[2][2]])<8 and s[0]>5: t0=s[0];break
        if t0 is None: continue
        def at(t):
            k=min(bisect.bisect_left(T,t),len(steps)-1); return steps[k]
        for ts in strokes:
            if not(t0<=ts<=t0+60): continue
            s0=at(ts)
            d0=math.hypot(s0[1][0]-s0[2][0],s0[1][2]-s0[2][2])
            if d0>2.8 or s0[1][13]<0.9 or s0[2][13]<0.9: continue
            u=((s0[2][0]-s0[1][0])/d0,(s0[2][2]-s0[1][2])/d0)
            row=[d0]
            for dt in (0.1,0.2,0.3,0.5,0.8,1.2):
                s=at(ts+dt); d=math.hypot(s[1][0]-s[2][0],s[1][2]-s[2][2]); row.append(d-d0)
            # velocity change along u after 0.3 s
            s3=at(ts+0.3)
            row.append((s3[1][3]-s0[1][3])*u[0]+(s3[1][5]-s0[1][5])*u[1])   # sentinel dv along u (toward WL)
            row.append((s3[2][3]-s0[2][3])*u[0]+(s3[2][5]-s0[2][5])*u[1])   # WL dv
            row.append(C.at(cm[0],ts)[0])
            R.append(row)
    print(name,len(R))
    q=lambda v:[round(sorted(v)[int(len(v)*f)],2) for f in (.1,.25,.5,.75,.9)]
    for k,lab in ((1,'dd@0.1'),(2,'dd@0.2'),(3,'dd@0.3'),(4,'dd@0.5'),(5,'dd@0.8'),(6,'dd@1.2'),(7,'dvS@0.3'),(8,'dvW@0.3')):
        print('  ',lab,q([r[k] for r in R]))
    return R
if __name__=='__main__':
    live=[]
    for i in range(4):
        steps=pickle.load(open('live2s_%d.pkl'%i,'rb')); E=pickle.load(open('live2_%d.pkl'%i,'rb'))
        cm,_=C.live_cmds('rp-out2/boxstart-sentinel-wideload-compressor__live__00%d.tsv'%i,steps)
        live.append((steps,E,E['BF'],cm))
    ours=[]
    for i in range(16):
        steps=pickle.load(open('ours/base/s%d.pkl'%i,'rb')); E=pickle.load(open('ours/base/e%d.pkl'%i,'rb'))
        ours.append((steps,E,strokes_ours(i),C.ours_cmds('ours/base/log%d.txt'%i)))
    go('live',live); go('ours',ours)
