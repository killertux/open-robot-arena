import sys; sys.path.insert(0,'.')
import pickle,math,bisect,subprocess,os,statistics as st,collections
import rp_cmd2 as C
G=os.environ['PRESSED_GAME']   # a scratch install (Teams, Robot Designs, AI/Teams.txt, ra2.cfg copied; the rest linked)
BIN=os.path.expanduser('~/Projects/ora-wt/pressed/engine/target/release/examples/pairreplay')
def quat(r):
    # columns r[0:3], r[3:6], r[6:9]
    m=[[r[0],r[3],r[6]],[r[1],r[4],r[7]],[r[2],r[5],r[8]]]  # m[row][col]
    t=m[0][0]+m[1][1]+m[2][2]
    if t>0:
        s=math.sqrt(t+1)*2; w=0.25*s; x=(m[2][1]-m[1][2])/s; y=(m[0][2]-m[2][0])/s; z=(m[1][0]-m[0][1])/s
    elif m[0][0]>m[1][1] and m[0][0]>m[2][2]:
        s=math.sqrt(1+m[0][0]-m[1][1]-m[2][2])*2; w=(m[2][1]-m[1][2])/s; x=0.25*s; y=(m[0][1]+m[1][0])/s; z=(m[0][2]+m[2][0])/s
    elif m[1][1]>m[2][2]:
        s=math.sqrt(1+m[1][1]-m[0][0]-m[2][2])*2; w=(m[0][2]-m[2][0])/s; x=(m[0][1]+m[1][0])/s; y=0.25*s; z=(m[1][2]+m[2][1])/s
    else:
        s=math.sqrt(1+m[2][2]-m[0][0]-m[1][1])*2; w=(m[1][0]-m[0][1])/s; x=(m[0][2]+m[2][0])/s; y=(m[1][2]+m[2][1])/s; z=0.25*s
    n=math.sqrt(x*x+y*y+z*z+w*w); return x/n,y/n,z/n,w/n
def starts(steps,E,gap=2.0,pre=0.3):
    t0=None
    for s in steps:
        if math.dist([s[1][0],s[1][2]],[s[2][0],s[2][2]])<8 and s[0]>5: t0=s[0];break
    C_=E['contact']; out=[]; last=-9
    T=[s[0] for s in steps]
    for t in C_:
        if t<t0 or t>t0+60 or t-last<gap: continue
        i=bisect.bisect_left(C_,t-pre)
        # contact continuously for the previous `pre` seconds
        ok=all(any(abs(c-(t-pre+k*0.05))<0.1 for c in C_[max(0,i-3):i+60]) for k in range(1,7))
        k=min(bisect.bisect_left(T,t),len(steps)-1); s=steps[k]
        if not ok or s[1][13]<0.9 or s[2][13]<0.9: continue
        if math.hypot(s[1][0]-s[2][0],s[1][2]-s[2][2])>2.9: continue
        out.append(t); last=t
    return out
def replay(steps,E,cm,ts,tag,secs=2.5,dtc=0.1):
    T=[s[0] for s in steps]
    k=min(bisect.bisect_left(T,ts),len(steps)-1); s=steps[k]
    tag=os.path.abspath(tag); sf=f'{tag}.state'; sc=f'{tag}.script'; so=f'{tag}.out'
    with open(sf,'w') as f:
        for ri,idx in ((0,1),(1,2)):
            q=s[idx]; qq=quat(q[9:18])
            f.write('%d %s %s %s %s %s\n'%(ri,' '.join('%.6f'%x for x in q[0:3]),' '.join('%.6f'%x for x in qq),' '.join('%.5f'%x for x in q[3:6]),' '.join('%.5f'%x for x in q[6:9]),''))
    fm=[t for t,c in E['FM']]; bf=E['BF']
    with open(sc,'w') as f:
        t=ts
        while t<ts+secs:
            a=C.at(cm[0],t); b=C.at(cm[1],t)
            fl=1 if any(t<=x<t+dtc for x in fm) else 0
            fi=1 if any(t<=x<t+dtc for x in bf) else 0
            f.write('%.3f %.1f %.1f %.1f %.1f %d %d\n'%(t-ts,a[0],a[1],b[0],b[1],fl,fi))
            t+=dtc
    r=subprocess.run([BIN,G+'/Arenas/compressor.py',G+'/AI/Team0/Bot2.bot',G+'/AI/Team8/Bot2.bot',sf,sc,str(secs),so],capture_output=True,text=True,cwd=G)
    if r.returncode!=0: print(r.stderr[-500:]); return None
    rows=[l.split('\t') for l in open(so)]
    return rows
def dist_series(rows):
    d={}
    byt=collections.defaultdict(dict)
    for r in rows:
        byt[float(r[0])][int(r[1])]=(float(r[2]),float(r[4]))
    return {t:math.hypot(v[0][0]-v[1][0],v[0][1]-v[1][1]) for t,v in byt.items() if 0 in v and 1 in v}
if __name__=='__main__':
    res=[]
    for i in range(4):
        steps=pickle.load(open('live2s_%d.pkl'%i,'rb')); E=pickle.load(open('live2_%d.pkl'%i,'rb'))
        cm,_=C.live_cmds('rp-out2/boxstart-sentinel-wideload-compressor__live__00%d.tsv'%i,steps)
        T=[s[0] for s in steps]
        for n,ts in enumerate(starts(steps,E)):
            rows=replay(steps,E,cm,ts,'rr/r%d_%d'%(i,n))
            if rows is None: continue
            ds=dist_series(rows)
            ts_=sorted(ds)
            def live_d(tt):
                k=min(bisect.bisect_left(T,tt),len(steps)-1); s=steps[k]; return math.hypot(s[1][0]-s[2][0],s[1][2]-s[2][2])
            def ours_d(tau): 
                k=min(range(len(ts_)),key=lambda j:abs(ts_[j]-tau)); return ds[ts_[k]]
            d0=live_d(ts)
            row=[i,ts,d0]+[(live_d(ts+tau)-d0,ours_d(tau)-d0) for tau in (0.5,1.0,1.5,2.0)]
            res.append(row)
    pickle.dump(res,open('replay_res.pkl','wb'))
    print(len(res))
    for j,tau in enumerate((0.5,1.0,1.5,2.0)):
        L=[r[3+j][0] for r in res]; O=[r[3+j][1] for r in res]
        print('tau',tau,'live dd mean %.2f med %.2f | ours dd mean %.2f med %.2f | share(dd>0.6) live %.2f ours %.2f'%(st.mean(L),st.median(L),st.mean(O),st.median(O),sum(x>0.6 for x in L)/len(L),sum(x>0.6 for x in O)/len(O)))
