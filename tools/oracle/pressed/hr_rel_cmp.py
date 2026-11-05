import json,glob,statistics as st,sys
O='/home/bruno/Projects/ora-wt/hitrate/local/oracle/original/'
def dot(a,b): return sum(x*y for x,y in zip(a,b))
def live(n):
    rows=[json.loads(l) for l in open(O+f"robotpair-sentinel-wideload-compressor__live__{n:03d}.jsonl")]
    G=[r[2] for r in rows if r[0]=='G']; wl,se=G[0],G[1]
    C=set(r[1] for r in rows if r[0]=='C' and r[2]); out=[]
    for r in rows:
        if r[0]=='S' and r[1] in C:
            d={b[0]:b for b in r[2]}
            if wl in d and se in d:
                a,b=d[se],d[wl]; dd=[a[1+i]-b[1+i] for i in range(3)]; rot=b[10:19]
                out.append([dot(dd,rot[0:3]),dot(dd,rot[3:6]),dot(dd,rot[6:9]),dd[1],a[10+4],b[10+4],(dd[0]**2+dd[2]**2)**.5])
    return out
L=[x for n in range(4,12) for x in live(n)]
Ou=[]
for f in glob.glob(sys.argv[1]+'/p*.txt'):
    for l in open(f): Ou.append([float(x) for x in l.split()[1:]])
names=['S in WL x','S in WL y','S in WL z','dy world','S up.y','WL up.y','planar dist']
for j,nm in enumerate(names):
    for lab,D in (('live',L),('ours',Ou)):
        v=sorted(x[j] for x in D); q=lambda p:v[int(p*(len(v)-1))]
        print('%-12s %s n=%d  p10 %.2f p25 %.2f med %.2f p75 %.2f p90 %.2f'%(nm,lab,len(v),q(.1),q(.25),q(.5),q(.75),q(.9)))
