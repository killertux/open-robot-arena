import ast,sys,math,collections
rows=[l.rstrip('\n').split('\t') for l in open(sys.argv[1])]
# GetSpeed rows for player 0 and 1 with extras
by=collections.defaultdict(list)
for r in rows:
    if len(r)>=6 and r[2]=='GetSpeed':
        t=float(r[0]); pid=r[1]; sp=float(r[4])
        d,loc,h,ud=ast.literal_eval(r[5])
        by[pid].append((t,sp,d,loc,h,ud))
for pid,L in by.items():
    print('player',pid,len(L))
    res_flat=[];res_raw=[];n=0
    big=[]
    for i in range(1,len(L)):
        t0,s0,d0,l0,h0,u0=L[i-1]; t1,s1,d1,l1,h1,u1=L[i]
        dt=t1-t0
        if dt<=0 or dt>0.3: continue
        v=[(l1[k]-l0[k])/dt for k in range(3)]
        # velocity estimate at the midpoint; compare with speed average
        sp=(s0+s1)/2
        dm=[(d0[k]+d1[k])/2 for k in range(3)]
        nm=math.sqrt(sum(x*x for x in dm))
        raw=sum(v[k]*dm[k] for k in range(3))
        fl=math.hypot(dm[0],dm[2]); flat=(v[0]*dm[0]+v[2]*dm[2])/fl if fl>1e-6 else 0
        res_raw.append(sp-raw); res_flat.append(sp-flat)
        if abs(dm[1])>0.3: big.append((t1,sp,raw,flat,dm[1]))
    def rms(a): return math.sqrt(sum(x*x for x in a)/len(a)) if a else 0
    print(' n',len(res_raw),'rms raw',rms(res_raw),'rms flat',rms(res_flat))
    print(' tilted samples',len(big))
    sr=[abs(b[1]-b[2]) for b in big]; sf=[abs(b[1]-b[3]) for b in big]
    if big: print(' tilted rms raw',rms([b[1]-b[2] for b in big]),'flat',rms([b[1]-b[3] for b in big]))
    for b in big[:12]: print('  t=%.2f speed=%.2f v.raw=%.2f v.flat=%.2f dir.y=%.2f'%b)
