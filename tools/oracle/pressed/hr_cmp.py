import json,glob,sys,statistics as st,collections
O='/home/bruno/Projects/ora-wt/hitrate/local/oracle/original/'
def live(n):
    cs=set(); am=[]; amr=[]
    for l in open(O+f"robotpair-sentinel-wideload-compressor__live__{n:03d}.jsonl"):
        r=json.loads(l)
        if r[0]=='C' and r[2]: cs.add(r[1])
        if r[0]=='H':
            if r[2]==1 and r[4]==0: am.append(r[6])
            if r[2]==0 and r[4]==1: amr.append(r[6])
    return len(cs),am,amr
def ours(d):
    out=[]
    for f in sorted(glob.glob(d+'/c*.txt')):
        w=f.split('/c')[-1][:-4]
        nc=sum(1 for _ in open(f))
        am=[];amr=[]
        for l in open(d+f'/h{w}.jsonl'):
            r=json.loads(l)
            if r['att']==1 and r['vic']==0: am.append(r['amount'])
            if r['att']==0 and r['vic']==1: amr.append(r['amount'])
        out.append((nc,am,amr))
    return out
def summ(name,runs):
    nc=sum(r[0] for r in runs); a=[x for r in runs for x in r[1]]; b=[x for r in runs for x in r[2]]
    print(f"{name}: fights {len(runs)} contact-steps {nc}  WL->S hits/step {len(a)/nc:.3f} (>=10: {sum(x>=10 for x in a)/nc:.4f}, >=30: {sum(x>=30 for x in a)/nc:.4f}, <1: {sum(x<1 for x in a)/nc:.3f})  S->WL {len(b)/nc:.3f} (>=10: {sum(x>=10 for x in b)/nc:.4f})")
    bins=[0,1,3,10,20,40,80,1e9]
    h=collections.Counter(); 
    for x in a:
        for i in range(len(bins)-1):
            if bins[i]<=x<bins[i+1]: h[i]+=1
    print('   WL->S amount bins per 1000 steps',[round(1000*h[i]/nc,1) for i in range(len(bins)-1)])
L=[live(n) for n in range(4,12)]
summ('live 4-11',L); summ('live 8-11',L[4:])
for d in sys.argv[1:]: summ(d,ours(d))
