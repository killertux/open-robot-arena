import json,sys,math,statistics as st
def sub(a,b): return [x-y for x,y in zip(a,b)]
def dot(a,b): return sum(x*y for x,y in zip(a,b))
def live(fn):
    rows=[json.loads(l) for l in open(fn)]
    R={};lastF={};ret=[]
    cur=None
    for r in rows:
        if r[0]=='R':
            bs=[b for b in r[2] if abs(b[0]-232)<1 or abs(b[0]-33)<1]
            if len(bs)==2: R[r[1]]=bs
        if r[0]=='F' and r[3]>0: ret.append(r[1])
    # strokes: group consecutive return F steps
    ends=[s for i,s in enumerate(ret) if i+1==len(ret) or ret[i+1]!=s+1]
    return R,ends,lambda b:(b[1:4],b[4:7])
def ours(fn):
    steps=[];cur=None
    for l in open(fn):
        if not l.startswith('["'): continue
        r=json.loads(l);k=r[0]
        if k=='R':
            if cur is not None: steps.append(cur)
            cur={'F':[]}
        elif k=='B' and cur is not None: cur[r[2]]=r
        elif k=='F' and r[2]=='ALARM' and cur is not None: cur['F'].append(r[4])
    if cur: steps.append(cur)
    ch=[k for k,v in steps[400].items() if k.startswith('h') and abs(v[3]-232)<1][0]
    ro=[k for k,v in steps[400].items() if k.startswith('h') and abs(v[3]-33)<1][0]
    R={i:[[0]+list(s[ch][4])+list(s[ch][5]),[0]+list(s[ro][4])+list(s[ro][5])] for i,s in enumerate(steps) if ch in s and ro in s}
    ret=[i for i,s in enumerate(steps) if any(f>0 for f in s['F'])]
    ends=[s for i,s in enumerate(ret) if i+1==len(ret) or ret[i+1]!=s+1]
    return R,ends,None
kind,fn=sys.argv[1:3]
R,ends,_=(live if kind=='live' else ours)(fn)
D=[]
for e in ends:
    if not all(e+k in R for k in range(0,5)): continue
    # axis: from chassis to rod at e (direction of rod travel is along chassis->rod offset at fire; use rod-chassis at e)
    ch,ro=R[e][0],R[e][1]
    d=sub(ro[1:4],ch[1:4]); L=math.sqrt(dot(d,d)); ax=[x/L for x in d]
    cv=[dot(R[e+k][0][4:7],ax) for k in range(5)]
    rel=[dot(sub(R[e+k][1][4:7],R[e+k][0][4:7]),ax) for k in range(5)]
    D.append((cv[1]-cv[0],cv[2]-cv[1],cv[3]-cv[2],cv[4]-cv[0],rel[0]))
print(kind,fn,len(D),'strokes')
for j,n in enumerate(['d1 limit','d2 lock','d3','tot0-4']):
    v=[x[j] for x in D]; print(n,'mean %.2f median %.2f sd %.2f'%(st.mean(v),st.median(v),st.pstdev(v)))
print('rod rel speed before',st.median([x[4] for x in D]))
