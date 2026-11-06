"""hit_to_tail.py [ensemble dirs]: per ALARM blow (>= 10 hp) on the Scout, the live fights and ours (ensemble.sh dirs with octagon/ and box/, FIDTRACE_UP=1): is the Scout on its tail at the blow, and put there by it (docs/29 12bf). Run from the repository root."""
import json,glob,math,ast,sys,statistics as st
O='local/oracle/original/'
def live(files):
    out=[]
    for f in files:
        t=0.0;S=[];H=[]
        for l in open(f):
            if l.startswith('["A"'): t+=json.loads(l)[2]
            elif l.startswith('["S"'): S.append((t,json.loads(l)[2]))
            elif l.startswith('["H"'):
                r=json.loads(l)
                if r[2]==1 and r[4]==0 and r[6]>=10: H.append((t,r[6]))
        first=S[0][1]
        idx=min(range(len(first)),key=lambda i:(first[i][1]+5.96)**2+(first[i][3]-5.75)**2) if 'octagon' in f else None
        if idx is None:
            # box: nearest to Scout start (-5.96,5.75)? use same
            idx=min(range(len(first)),key=lambda i:(first[i][1]+5.96)**2+(first[i][3]-5.75)**2)
        ts=[x[0] for x in S]; ys=[x[1][idx][14] for x in S]
        bell=None
        for tt,ch in S:
            v=math.sqrt(sum(x*x for x in ch[idx][4:7]))
            if v>0.8 and ch[idx][2]<0.6: bell=tt-0.3;break
        out.append((ts,ys,[h for h in H if h[0]>=bell and h[0]-bell<=120]))
    return out
def ours(d):
    out=[]
    for f in sorted(glob.glob(d+'/t*.tsv')):
        w=f.split('/t')[-1][:-4]
        ts=[];ys=[]
        for line in open(f).read().split('\n')[1:]:
            if line.strip():
                t,r=line.split('\t'); a,_=ast.literal_eval(r)
                ts.append(float(t)); ys.append(a[-1])
        H=[]
        for l in open(d+'/h%s.jsonl'%w):
            r=json.loads(l)
            if r['att']==1 and r['vic']==0 and r['amount']>=10 and r['t']<=120: H.append((r['t'],r['amount']))
        out.append((ts,ys,H))
    return out
def stats(name,runs):
    n=0;tail=[0,0,0,0];mins=[];pre=0;nb=0;caused=0;rec=0
    import bisect
    for ts,ys,H in runs:
        last=-9
        for t,a in H:
            if t-last<0.5: continue   # one blow = several priced hits
            last=t
            n+=1
            ib=min(bisect.bisect_left(ts,t-0.05),len(ts)-1); i1=min(bisect.bisect_left(ts,t+1.0),len(ts)-1)
            if abs(ys[ib])<0.3:
                pre+=1; rec+= abs(ys[i1])>=0.3
            else:
                nb+=1; caused+= abs(ys[i1])<0.3
            for k,dt in enumerate((0.25,0.5,1.0,2.0)):
                i=min(bisect.bisect_left(ts,t+dt),len(ts)-1)
                tail[k]+= abs(ys[i])<0.3
            j0=bisect.bisect_left(ts,t); j1=bisect.bisect_left(ts,t+2)
            mins.append(min(ys[j0:j1]) if j1>j0 else 1)
    print('      already on tail at the blow: %.1f%% (of those, off the tail 1 s later: %.0f%%); put on the tail by a blow on a non-tail Scout (tail at +1 s): %.1f%% of %d'%(100*pre/n,100*rec/max(pre,1),100*caused/max(nb,1),nb))
    print('%-5s blows %4d  on tail after 0.25/0.5/1/2 s: %s   min up.y<0.3 within 2 s: %.0f%%   median min %.2f'%(name,n,' '.join('%.1f%%'%(100*x/n) for x in tail),100*sum(m<0.3 for m in mins)/n,st.median(mins)))
for arena in ('octagon','box'):
    if arena=='octagon':
        L=sorted(glob.glob(O+'sidefight-scout-alarm-octagon__live__0*.jsonl'))+[O+'boxstart-actorder-scout-alarm-octagon__live__000.jsonl']
    else:
        L=sorted(glob.glob(O+'sidefight-scout-alarm-box__live__0*.jsonl'))+[O+'boxstart-actorder-scout-alarm-box__live__000.jsonl']
    print(arena)
    stats('live',live(L))
    for m in sys.argv[1:]:
        stats(m.split('/')[-1],ours(m+'/'+arena))
