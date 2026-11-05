import sys; sys.path.insert(0,'.')
import pickle,math,bisect,statistics as st
def live_series(E):
    out=[]
    for t,c,dr,lk,al,bodies in E['U']:
        if not bodies: continue
        ch=[b for b in bodies if abs(b[2]-226.67)<1]; rd=[b for b in bodies if abs(b[2]-63)<1]
        if ch and rd:
            a=ch[0][3]; b=rd[0][3]; out.append((t,math.dist(a,b),rd[0][4],ch[0][4]))
    return out
def ours_series(E):
    ch={};out=[]
    for t,name,com,vel in E['rod']:
        if name=='h11': ch[t]=(com,vel)
        else:
            c=ch.get(t)
            if c: out.append((t,math.dist(c[0],com),vel,c[1]))
    return out
def go(name,series_list,fires_list,presses):
    allc=[]
    for ser,fires,pr in zip(series_list,fires_list,presses):
        T=[x[0] for x in ser]
        base=sorted(x[1] for x in ser)[len(ser)//10]
        for tf in fires:
            if not pr(tf): continue
            row=[]
            for dt in (0.0,0.05,0.1,0.15,0.2,0.25,0.3,0.4,0.5,0.7,1.0):
                k=min(bisect.bisect_left(T,tf+dt),len(ser)-1); row.append(ser[k][1]-base)
            allc.append(row)
    n=len(allc)
    print(name,n)
    print('   mean ext',[round(st.mean(r[k] for r in allc),2) for k in range(11)])
    print('   med  ext',[round(st.median(r[k] for r in allc),2) for k in range(11)])
    print('   frac ext>0.15 at .5 s',round(sum(r[8]>0.15 for r in allc)/n,2),' at 0.7:',round(sum(r[9]>0.15 for r in allc)/n,2),' at 1.0:',round(sum(r[10]>0.15 for r in allc)/n,2))
if __name__=='__main__':
  LS=[];LF=[];LP=[]; OS=[];OF=[];OP=[]
  for i in range(4):
      E=pickle.load(open('live2_%d.pkl'%i,'rb')); LS.append(live_series(E)); LF.append(E['BF']); C=E['contact']
      LP.append(lambda t,C=C:(lambda k:k<len(C) and C[k]<=t+0.2)(bisect.bisect_left(C,t-0.2)))
  for i in range(16):
      E=pickle.load(open('ours/base/e%d.pkl'%i,'rb')); OS.append(ours_series(E)); OF.append([t for t,f in E['F'] if f==-1000]); C=E['contact']
      OP.append(lambda t,C=C:(lambda k:k<len(C) and C[k]<=t+0.2)(bisect.bisect_left(C,t-0.2)))
  for lab,sel in (('pressed at fire',True),('free at fire',False)):
      go('live '+lab,LS,LF,[ (lambda t,p=p:p(t)==sel) for p in LP]); go('ours '+lab,OS,OF,[ (lambda t,p=p:p(t)==sel) for p in OP])
  
