import pickle,glob,math,sys,collections,statistics as st
def load(pat): return [pickle.load(open(p,'rb')) for p in sorted(glob.glob(pat))]
def loc(q,w):  # world vector -> local (rows)
    r=q[9:18]; return [r[i*3]*w[0]+r[i*3+1]*w[1]+r[i*3+2]*w[2] for i in range(3)]
def sub(a,b): return [a[i]-b[i] for i in range(3)]
def norm(v): return math.sqrt(sum(x*x for x in v))
def analyse(steps,live):
    # normalise contacts so robot S first: returns list of (t,dist,ctsS...) 
    # robot id mapping for live
    side=None
    if live:
        v=collections.Counter()
        for s_ in steps:
            for c in s_[3]:
                dv=sub(s_[2][0:3],s_[1][0:3]); dot=sum(c[4][i]*dv[i] for i in range(3))
                v[(c[2],c[3],dot>0)]+=1
        print('   vote',dict(v))
        r2=v[(2,1,True)]+v[(1,2,False)]; r1=v[(2,1,False)]+v[(1,2,True)]
        side={2:1,1:0} if r2>=r1 else {2:0,1:1}
    # window: first step with centre dist<8, +60s
    t0=None
    for s in steps:
        if norm(sub(s[1][0:3],s[2][0:3]))<8 and s[0]>5: t0=s[0]; break
    out=[]
    for s in steps:
        if t0 is None or not(t0<=s[0]<=t0+60): continue
        if s[1][13]<0.7 or s[2][13]<0.7: continue
        cs=[]
        for c in s[3]:
            if live:
                a,b=side[c[2]],side[c[3]]
                if a==b: continue
                if a==0: cs.append((c[0],c[1],tuple(-x for x in c[4]),c[5],c[6]))
                else: cs.append((c[1],c[0],c[4],c[5],c[6]))
            else:
                if c[2]==0: cs.append((c[0],c[1],tuple(-x for x in c[4]),c[5],c[6]))
                else: cs.append((c[1],c[0],c[4],c[5],c[6]))
        out.append((s,cs))
    return out
def summarize(name,runs,live):
    occ=[];n_c=[];brgS=[];brgW=[];dist=[];closing=[];ncont=[];hS=[]
    ptl=collections.Counter();nrm=collections.Counter()
    for steps in runs:
        W=analyse(steps,live)
        if not W: continue
        inc=0
        for s,cs in W:
            if cs: inc+=1
            if not cs: continue
            se,wl=s[1],s[2]
            d=sub(wl[0:3],se[0:3]); dd=norm(d); dist.append(dd)
            ls=loc(se,d); lw=loc(wl,[-x for x in d])
            brgS.append(math.degrees(math.atan2(ls[0],ls[2]))); brgW.append(math.degrees(math.atan2(lw[0],lw[2])))
            ncont.append(len(cs))
            u=[d[0]/dd,0,d[2]/dd]
            closing.append(sum((wl[3+i]-se[3+i])*u[i] for i in (0,2)))
            for c in cs:
                p=loc(se,sub(c[3],se[0:3]))
                ptl[(round(p[0]/0.5)*0.5,round(p[2]/0.5)*0.5)]+=1
                n=loc(se,c[2]); nrm[round(math.degrees(math.atan2(n[0],n[2]))/30)*30]+=1
        occ.append(inc/len(W))
    def q(v): v=sorted(v); return [round(v[int(len(v)*f)],2) for f in (.1,.5,.9)] if v else None
    print(name,'runs',len(runs),'occ',round(sum(occ)/len(occ),3),'ncont mean',round(st.mean(ncont),2),'dist q',q(dist),'closing q',q(closing))
    tot=len(brgS)
    def hist(v):
        h=collections.Counter(int(((x+180)%360)//45) for x in v)
        return [round(h[i]/tot,2) for i in range(8)]
    print('  bearing of WL in S frame (bins from -180 by 45):',hist(brgS))
    print('  bearing of S in WL frame:',hist(brgW))
    tn=sum(nrm.values()); print('  normal(S frame) angle bins:',{k:round(v/tn,2) for k,v in sorted(nrm.items())})
    tp=sum(ptl.values()); print('  contact pt in S (x,z) top:',[(k,round(v/tp,2)) for k,v in ptl.most_common(8)])
if __name__=='__main__':
    summarize('live',load('live*.pkl'),True)
    summarize('ours',load('ours/%s/s*.pkl'%(sys.argv[1] if len(sys.argv)>1 else 'base')),False)
