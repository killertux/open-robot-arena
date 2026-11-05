import json,glob,collections,sys
O='/home/bruno/Projects/ora-wt/hitrate/local/oracle/original/'
def dot(a,b): return sum(x*y for x,y in zip(a,b))
def live(n):
    rows=[json.loads(l) for l in open(O+f"robotpair-sentinel-wideload-compressor__live__{n:03d}.jsonl")]
    G=[r[2] for r in rows if r[0]=='G']; wl=G[0]
    S={};cs=set();hits=[]
    for r in rows:
        if r[0]=='S':
            d={b[0]:b for b in r[2]}
            if wl in d: S[r[1]]=d[wl]
        if r[0]=='C' and r[2]: cs.add(r[1])
        if r[0]=='H' and r[2]==1 and r[4]==0 and r[1] in S:
            b=S[r[1]]; d=[r[7][i]-b[1+i] for i in range(3)]; rot=b[10:19]
            hits.append((r[6],[dot(d,rot[0:3]),dot(d,rot[3:6]),dot(d,rot[6:9])]))
    return len(cs),hits
def ours(d):
    out=[]
    for f in sorted(glob.glob(d+'/c*.txt')):
        w=f.split('/c')[-1][:-4]; nc=sum(1 for _ in open(f)); hits=[]
        for l in open(d+f'/h{w}.jsonl'):
            r=json.loads(l)
            if r['att']==1 and r['vic']==0: hits.append((r['amount'],r['la']))
        out.append((nc,hits))
    return out
def hist(runs,th):
    nc=sum(r[0] for r in runs); h=collections.Counter()
    for r in runs:
        for a,l in r[1]:
            if a>=th: h[(round(l[2]*2)/2, 'L' if l[0]<0 else 'R')]+=1   # forward axis z bins of 0.5
    return nc,h
L=[live(n) for n in range(4,12)]; Ou=ours(sys.argv[1])
th=float(sys.argv[2])
nl,hl=hist(L,th); no,ho=hist(Ou,th)
keys=sorted(set(hl)|set(ho))
print('threshold',th,'steps live',nl,'ours',no)
for k in keys:
    print(k,'live %.2f ours %.2f per 1000 steps'%(1000*hl[k]/nl,1000*ho[k]/no))
