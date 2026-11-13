"""length_window_cmp.py <pair> <ensemble dir of that pair> <side>: counted hits (>= 10 hp) of one attacker in the first T s
of fights that last at least T s, bell-primed live against ours, and the fight-length distributions (docs/29 12bh)."""
import sys,json,glob,re,statistics as st
sys.path.insert(0,__import__('os').path.dirname(__import__('os').path.abspath(__file__)))
import live_ref as L
pair=sys.argv[1]; d=sys.argv[2]; side=int(sys.argv[3])
live=[r for r in L.live_rows(pair) if r['cls']=='bell']
ours=[]
for h in glob.glob(d+'/h*.jsonl'):
    w=re.findall(r'h(\d+)\.jsonl',h)[0]
    log=[x for x in open(d+'/log%s.txt'%w) if x.startswith('fight')]
    ln=float(log[0].split()[1].rstrip('s,'))
    hs=[json.loads(x) for x in open(h)]
    ours.append((ln,[(x['t'],x['att'],x['amount']) for x in hs]))
def ci(v): return '%.2f+-%.2f'%(st.mean(v), st.stdev(v)/len(v)**.5 if len(v)>1 else 0)
for T in (20,40,60,90):
    lv=[];ov=[]
    for r in live:
        end=r['end'] if r['end'] else 999
        if end<T: continue
        c,_=L.summarise(r['hs'],0,T,r['shift']); lv.append(c[side])
    for ln,hs in ours:
        if ln<T: continue
        ov.append(sum(1 for t,a,m in hs if a==side and m>=10 and t<=T))
    print('window 0-%d s, fights lasting that long: live n=%d %s  ours n=%d %s'%(T,len(lv),ci(lv) if lv else '-',len(ov),ci(ov) if ov else '-'))
ll=[ (r['end'] or 125) for r in live]
print('length live',ci(ll),'ours',ci([l for l,_ in ours]))
print('frac <40s live %.2f ours %.2f'%(sum(1 for x in ll if x<40)/len(ll), sum(1 for l,_ in ours if l<40)/len(ours)))
print('frac <60s live %.2f ours %.2f'%(sum(1 for x in ll if x<60)/len(ll), sum(1 for l,_ in ours if l<60)/len(ours)))
