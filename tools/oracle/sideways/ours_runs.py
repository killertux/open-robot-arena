"""ours_runs.py <ensemble dir> [min seconds]: the Scout's runs on its tail in an ours ensemble with the AI tactic and throttle during each (RA2_AI_TRACE=1 FIDTRACE_UP=1 logs; docs/29 12bf)."""
import ast,re,sys,glob,collections
d=sys.argv[1]; thr=0.3; minlen=float(sys.argv[2]) if len(sys.argv)>2 else 3.0
tact=collections.Counter(); n=0
for f in sorted(glob.glob(d+'/t*.tsv')):
    w=f.split('/t')[-1][:-4]
    ts=[];ys=[]
    for line in open(f).read().split('\n')[1:]:
        if line.strip():
            t,r=line.split('\t')
            if float(t)>120: break
            a,_=ast.literal_eval(r); ts.append(float(t)); ys.append(a[-1])
    runs=[];s0=None
    for t,y in zip(ts,ys):
        if abs(y)<thr and s0 is None: s0=t
        if abs(y)>=thr and s0 is not None: runs.append((s0,t)); s0=None
    if s0 is not None: runs.append((s0,ts[-1]))
    long=[r for r in runs if r[1]-r[0]>=minlen]
    if not long: continue
    ai=[]
    for l in open(d+'/log%s.txt'%w):
        m=re.match(r'ai t=([\d.]+) bot=0 \(([-\d.]+),([-\d.]+)\) h=([-\d.]+) v=([-\d.]+) w=([-\d.]+) tactic=(\S+) throttle=([-\d.e]+) turn=([-\d.e]+)',l)
        if m: ai.append((float(m.group(1)),m.group(7),float(m.group(8)),float(m.group(9)),float(m.group(2)),float(m.group(3))))
    # clock offset: the first time the Scout is 0.3 m from its start in the two records
    sx,sz=None,None
    tm=None
    for line in open(f).read().split('\n')[1:]:
        if line.strip():
            t,r=line.split('\t'); a_,_=ast.literal_eval(r)
            if sx is None: sx,sz=a_[1][0],a_[1][2]
            if ((a_[1][0]-sx)**2+(a_[1][2]-sz)**2)**.5>0.3: tm=float(t); break
    lm=None
    for x in ai:
        if ((x[4]-ai[0][4])**2+(x[5]-ai[0][5])**2)**.5>0.3: lm=x[0]; break
    off=(lm-tm) if (lm is not None and tm is not None) else 0
    for a,b in long:
        seg=[x for x in ai if a+off<=x[0]<=b+off]
        c=collections.Counter(x[1] for x in seg)
        thr_=[x[2] for x in seg]; tu=[x[3] for x in seg]
        print('w%s run %.1f..%.1f (%.1fs) tactics %s throttle mean %.0f turn mean %.0f pos (%.1f,%.1f) r=%.1f'%(w,a,b,b-a,dict(c),sum(thr_)/max(1,len(thr_)),sum(tu)/max(1,len(tu)),seg[0][4] if seg else 0,seg[0][5] if seg else 0,(seg[0][4]**2+seg[0][5]**2)**.5 if seg else 0))
