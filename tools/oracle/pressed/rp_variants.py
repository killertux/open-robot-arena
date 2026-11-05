import sys,os; sys.path.insert(0,'.')
import pickle,math,bisect,statistics as st,collections,subprocess
import rp_replay as R, rp_cmd2 as C
SUBSET=bool(os.environ.get('SUBSET'))
STARTS=[]
for i in range(4):
    steps=pickle.load(open('live2s_%d.pkl'%i,'rb')); E=pickle.load(open('live2_%d.pkl'%i,'rb'))
    cm,_=C.live_cmds('rp-out2/boxstart-sentinel-wideload-compressor__live__00%d.tsv'%i,steps)
    T=[s[0] for s in steps]
    for n,ts in enumerate(R.starts(steps,E)): STARTS.append((i,n,ts,steps,T,E,cm))
def liv(steps,T,t):
    k=min(bisect.bisect_left(T,t),len(steps)-1); return steps[k]
def strong(i,n,ts,E):
    from rp_str import strengths
    S=strengths(E['F'])
    return any(ts<=t<ts+1.5 and d>=95 for t,d in S) and not any(ts<=t<ts+1.5 and d<95 for t,d in S)
def run(name,env=None,mod=None,bin=None):
    envd=dict(os.environ); envd.update(env or {})
    res=[]
    for i,n,ts,steps,T,E,cm in STARTS:
        if SUBSET and not strong(i,n,ts,E): continue
        tag=os.path.abspath('rr/v_%s_%d_%d'%(name,i,n)); sf=tag+'.state'; sc=tag+'.script'; so=tag+'.out'
        src='rr/r%d_%d'%(i,n); src=os.path.abspath(src)
        # reuse state/script of the base replay
        import shutil; shutil.copy(src+'.state',sf)
        lines=open(src+'.script').read().strip().split('\n')
        if mod: lines=[mod(l) for l in lines]
        open(sc,'w').write('\n'.join(lines)+'\n')
        r=subprocess.run([bin or R.BIN,R.G+'/Arenas/compressor.py',R.G+'/AI/Team0/Bot2.bot',R.G+'/AI/Team8/Bot2.bot',sf,sc,'2.5',so],capture_output=True,text=True,cwd=R.G,env=envd)
        if r.returncode!=0: print(r.stderr[-300:]); continue
        byt=collections.defaultdict(dict)
        for l in open(so):
            f=l.split('\t'); byt[round(float(f[0]),3)][int(f[1])]=(float(f[2]),float(f[4]))
        ots=sorted(byt)
        s0=liv(steps,T,ts); d0=math.hypot(s0[1][0]-s0[2][0],s0[1][2]-s0[2][2])
        row=[]
        for tau in (0.5,1.0,1.5,2.0):
            b=byt[min(ots,key=lambda x:abs(x-tau))]
            row.append(math.hypot(b[0][0]-b[1][0],b[0][1]-b[1][1])-d0)
        res.append(row)
    print('%-14s n=%d '%(name,len(res))+' | '.join('t%.1f med %.2f sep>0.6 %.2f'%(tau,st.median(r[j] for r in res),sum(r[j]>0.6 for r in res)/len(res)) for j,tau in enumerate((0.5,1.0,1.5,2.0))))
    return res
def live_row():
    res=[]
    for i,n,ts,steps,T,E,cm in STARTS:
        if SUBSET and not strong(i,n,ts,E): continue
        s0=liv(steps,T,ts); d0=math.hypot(s0[1][0]-s0[2][0],s0[1][2]-s0[2][2])
        res.append([math.hypot(*[liv(steps,T,ts+tau)[1][0]-liv(steps,T,ts+tau)[2][0],liv(steps,T,ts+tau)[1][2]-liv(steps,T,ts+tau)[2][2]])-d0 for tau in (0.5,1.0,1.5,2.0)])
    print('%-14s n=%d '%('LIVE',len(res))+' | '.join('t%.1f med %.2f sep>0.6 %.2f'%(tau,st.median(r[j] for r in res),sum(r[j]>0.6 for r in res)/len(res)) for j,tau in enumerate((0.5,1.0,1.5,2.0))))
def nofire(l):
    f=l.split(); f[6]='0'; return ' '.join(f)
def noflip(l):
    f=l.split(); f[5]='0'; return ' '.join(f)
def nopress(l):
    f=l.split(); f[5]='0'; f[6]='0'; return ' '.join(f)
def sidle(l):
    f=l.split(); f[1]='0'; f[2]='0'; return ' '.join(f)
if __name__=='__main__':
    live_row()
    for spec in sys.argv[1:]:
        name,_,rest=spec.partition(':')
        env={};mod=None
        for part in rest.split(','):
            if not part: continue
            if '=' in part: k,v=part.split('=');env[k]=v
            else: mod=globals()[part]
        run(name,env,mod)
