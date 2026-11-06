#!/usr/bin/env python3
"""inertia_cmp.py: ours (examples/bodycmp, RA2_INERTIA=old|new) against the live P rows of every capture (mass at +0x190, local inertia at +0x19c):
per body of Scout, ALARM, Sentinel, Razor, FlapJack, Wide Load the principal moments and the largest relative difference (docs/29 12bf).
Run from the repository root after building examples/bodycmp (needs local/game and local/oracle/original)."""
import json,glob,subprocess,re,os,sys
import math
def eig3(A):
    p1=A[0][1]**2+A[0][2]**2+A[1][2]**2
    q=(A[0][0]+A[1][1]+A[2][2])/3
    if p1<1e-18: return sorted([A[0][0],A[1][1],A[2][2]])
    p2=(A[0][0]-q)**2+(A[1][1]-q)**2+(A[2][2]-q)**2+2*p1
    p=math.sqrt(p2/6)
    B=[[(A[i][j]-(q if i==j else 0))/p for j in range(3)] for i in range(3)]
    det=(B[0][0]*(B[1][1]*B[2][2]-B[1][2]*B[2][1])-B[0][1]*(B[1][0]*B[2][2]-B[1][2]*B[2][0])+B[0][2]*(B[1][0]*B[2][1]-B[1][1]*B[2][0]))/2
    r=max(-1,min(1,det)); phi=math.acos(r)/3
    e1=q+2*p*math.cos(phi); e3=q+2*p*math.cos(phi+2*math.pi/3); e2=3*q-e1-e3
    return sorted([e1,e2,e3])
ROOT=os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)),'..','..','..'))
G=ROOT+'/local/game'
ORIG=ROOT+'/local/oracle/original'
bots={'Scout':'AI/Team0/Bot0.bot','ALARM':'AI/Team0/Bot1.bot','Sentinel':'AI/Team0/Bot2.bot','Razor':'AI/Team10/Bot1.bot','FlapJack':'AI/Team8/Bot0.bot','WideLoad':'AI/Team8/Bot2.bot'}
fams=['alarmfight','boxfight','fightcontacts','contacts','tipab','twobody','spawn','flipfight','gjkquery','flextable','audio','boxstart','robotpair']
live={}  # (round mass) -> set of tensors
for f in glob.glob(ORIG+'/*.jsonl'):
    for l in open(f):
        if not l.startswith('["P"'): continue
        r=json.loads(l)
        if len(r)!=4 or not isinstance(r[3],list) or len(r[3])<120: continue
        fl=r[3]; m=fl[0x190//4]
        if not (0.5<m<1000): continue
        I=[[fl[0x19c//4+4*i+j] for j in range(3)] for i in range(3)]
        if not all(math.isfinite(x) for r_ in I for x in r_) or abs(I[0][0]+I[1][1]+I[2][2])<1e-6: continue
        live.setdefault(round(m,2),{})[tuple(round(x,3) for x in eig3(I))]=eig3(I)
def ours(bot,mode):
    env=dict(os.environ,RA2_INERTIA=mode)
    out=subprocess.run([ROOT+'/engine/target/release/examples/bodycmp',G+'/Arenas/box.py',G+'/'+bots[bot]],capture_output=True,text=True,env=env).stdout
    res=[];cur=None
    for ln in out.splitlines():
        m=re.match(r'part (\d+) m ([\d.]+) (\[.*\])',ln)
        if m: cur=[int(m.group(1)),float(m.group(2)),m.group(3)]
        m=re.match(r'\s+Ibody diag \[([^\]]*)\] off\(xy,xz,yz\) \[([^\]]*)\]',ln)
        if m and cur:
            d=[float(x) for x in m.group(1).split(',')];o=[float(x) for x in m.group(2).split(',')]
            I=[[d[0],o[0],o[1]],[o[0],d[1],o[2]],[o[1],o[2],d[2]]]
            res.append((cur[0],cur[1],cur[2],I));cur=None
    return res
tot={}
for bot in bots:
    print('==',bot)
    for mode in ('old','new'):
        for (k,m,names,I) in ours(bot,mode):
            ev=eig3(I)
            cands=live.get(round(m,2),{})
            if not cands: 
                if mode=='new': print(f'  part {k} {m:8.3f} {names[:50]:50s} no live')
                continue
            best=min(cands.items(),key=lambda kv: max(abs(a-b) for a,b in zip(kv[1],ev)))
            lev=best[1]; rel=[(a-b)/max(b,1e-6) for a,b in zip(ev,lev)]
            tot.setdefault(mode,[]).append(max(abs(x) for x in rel))
            print(f'  {mode} part {k} {m:8.3f} {names[:46]:46s} ours {[round(x,4) for x in ev]} live {[round(x,4) for x in lev]} maxrel {max(abs(x) for x in rel)*100:6.2f}%')
for m,v in tot.items(): print(m,'bodies',len(v),'mean maxrel %.3f%%'%(100*sum(v)/len(v)),'max %.2f%%'%(100*max(v)),'within0.1%%: %d'%sum(1 for x in v if x<0.001))
