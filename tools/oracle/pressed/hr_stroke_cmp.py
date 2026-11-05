import json,glob,bisect,collections
O='/home/bruno/Projects/ora-wt/hitrate/local/oracle/original/'
def live(n):
    t=0;T={};fires=[];hits=[];cont=[]
    for l in open(O+f"robotpair-sentinel-wideload-compressor__live__{n:03d}.jsonl"):
        r=json.loads(l)
        if r[0]=='A': t+=r[2]
        elif r[0]=='BF': fires.append(t)
        elif r[0]=='H' and r[2]==1 and r[4]==0: hits.append((t,r[6]))
        elif r[0]=='C' and r[2]: cont.append(t)
    return fires,hits,cont
def ours(d):
    out=[]
    for f in sorted(glob.glob(d+'/f*.txt')):
        w=f.split('/f')[-1][:-4]
        fires=[float(l.split()[0]) for l in open(f) if ' Z ' not in l and l.split()[1]=='1' and l.split()[4]=='1']
        hits=[]
        for l in open(d+f'/h{w}.jsonl'):
            r=json.loads(l)
            if r['att']==1 and r['vic']==0: hits.append((r['t'],r['amount']))
        cont=[float(l.split()[0]) for l in open(d+f'/c{w}.txt')]
        out.append((fires,hits,cont))
    return out
def stats(runs,name):
    ns=0;h10=0;h3=0;hall=0;withc=0;late10=0;nfire_contact=0;tot_c=0
    for fires,hits,cont in runs:
        ht=[h[0] for h in hits]
        for f in fires:
            ns+=1
            win=[h for h in hits if f<=h[0]<f+0.6]
            hall+=len(win); h10+=sum(a>=10 for _,a in win); h3+=sum(a>=3 for _,a in win)
            i=bisect.bisect_left(cont,f-0.2); j=bisect.bisect_right(cont,f+0.6)
            if j-i>0: nfire_contact+=1
        tot_c+=len(cont)
    print(f"{name}: strokes {ns}  with robot contact within -0.2..+0.6 s: {nfire_contact/ns:.2f}  per stroke: hits {hall/ns:.2f} >=3: {h3/ns:.2f} >=10: {h10/ns:.2f}")
    # and per stroke only those with contact
    ns=h10=h3=0
    for fires,hits,cont in runs:
        for f in fires:
            i=bisect.bisect_left(cont,f-0.2); j=bisect.bisect_right(cont,f+0.6)
            if j-i>0:
                ns+=1; win=[h for h in hits if f<=h[0]<f+0.6]; h10+=sum(a>=10 for _,a in win); h3+=sum(a>=3 for _,a in win)
    print(f"    strokes in contact {ns}: >=3 per stroke {h3/ns:.2f}, >=10 per stroke {h10/ns:.2f}")
L=[live(n) for n in range(4,12)]
stats(L,'live 4-11'); stats(L[4:],'live 8-11'); stats(ours('hr_ours/new5'),'ours')

def per_stroke(runs,name):
    firsts=[];maxs=[];dts=[];n_ev=collections.Counter()
    for fires,hits,cont in runs:
        for f in fires:
            win=sorted(h for h in hits if f<=h[0]<f+0.6)
            if not win: continue
            firsts.append(win[0][1]); dts.append(win[0][0]-f); maxs.append(max(a for _,a in win)); n_ev[min(len(win),6)]+=1
    def q(v,p): v=sorted(v); return v[int(p*(len(v)-1))]
    print(name,'strokes with event',len(firsts))
    print('  first event amount p25 %.1f med %.1f p75 %.1f p90 %.1f'%tuple(q(firsts,p) for p in (.25,.5,.75,.9)))
    print('  max amount in stroke p25 %.1f med %.1f p75 %.1f p90 %.1f'%tuple(q(maxs,p) for p in (.25,.5,.75,.9)))
    print('  first event delay after fire p10 %.2f med %.2f p90 %.2f'%tuple(q(dts,p) for p in (.1,.5,.9)))
    print('  events per stroke dist',[round(n_ev[k]/len(firsts),2) for k in range(1,7)])
per_stroke(L,'live 4-11'); per_stroke(L[4:],'live 8-11'); per_stroke(ours('hr_ours/new5'),'ours')
