#!/bin/bash
# hr_ours.sh <warmup> <tag>: one Compressor fight of ours with RA2_H17_DUMP and FIDTRACE_HITS; the dump is reduced to
# the resolver entries with a contact between the two robots (docs/29 12az). HR_WORK a scratch dir, HR_GAME a scratch install.
w=$1; tag=$2; S=${HR_WORK:?}; G=${HR_GAME:?}
BIN=${BIN:-$HOME/Projects/ora-wt/hitrate/engine/target/release/examples/fidtrace}
mkdir -p $S/$tag
RA2_AI_TRACE=1 FIDTRACE_WARMUP=$w FIDTRACE_UP=1 FIDTRACE_HITS=$S/$tag/h$w.jsonl FIDTRACE_FIRE=$S/$tag/f$w.txt RA2_H17_DUMP=$S/$tag/d$w.jsonl $BIN match $G/Arenas/compressor.py $G/AI/Team0/Bot2.bot $G/AI/Team8/Bot2.bot ${SECS:-125} $S/$tag/t$w.tsv $S/$tag/dd$w.jsonl > $S/$tag/log$w.txt 2>&1
python3 - $S/$tag/d$w.jsonl $S/$tag/c$w.txt $S/$tag/e$w.txt $S/$tag/k$w.txt $S/$tag/p$w.txt <<'P'
import sys,json
out=open(sys.argv[2],'w'); ev=open(sys.argv[3],'w'); cp=open(sys.argv[3].replace('e','k',1) if False else sys.argv[4],'w'); rel=open(sys.argv[5],'w'); Bs={}; nowc=False; t=0.0; pend=None
rob=lambda i:0 if 5<=i<=10 else (1 if 11<=i<=16 else -1)
for l in open(sys.argv[1]):
    if not l.startswith('["'): continue
    r=json.loads(l); k=r[0]
    if k=='A': t+=r[2]
    elif k=='EV':
        a,b=r[2],r[3]
        if rob(a)>=0 and rob(b)>=0 and rob(a)!=rob(b): ev.write('%d %d %d %d %d\n'%(r[4],r[5],r[6],rob(a),rob(b)))
    elif k=='R':
        n=sum(1 for c in r[3] if rob(int(c[0][1:]))>=0 and rob(int(c[1][1:]))>=0 and rob(int(c[0][1:]))!=rob(int(c[1][1:])))
        if n: out.write('%.4f %d\n'%(t,n))
        nowc=bool(n); Bs={}
        pend=[c for c in r[3] if rob(int(c[0][1:]))>=0 and rob(int(c[1][1:]))>=0 and rob(int(c[0][1:]))!=rob(int(c[1][1:]))]
    elif k=='B' and r[2] in ('h5','h11'):
        Bs[r[2]]=r
        if r[2]=='h11' and 'h5' in Bs and nowc:
            a,b=Bs['h5'],Bs['h11']; d=[a[4][i]-b[4][i] for i in range(3)]; rot=b[7]
            rel.write('%.4f %.3f %.3f %.3f %.3f %.3f %.3f %.3f %.3f %.3f %.3f\n'%(t,sum(d[i]*rot[i] for i in range(3)),sum(d[i]*rot[3+i] for i in range(3)),sum(d[i]*rot[6+i] for i in range(3)),d[1],a[7][4],b[7][4],(d[0]**2+d[2]**2)**.5,a[7][5],b[7][5],a[4][1]-b[4][1]))
            nowc=False
    if k=='B' and r[2]=='h5' and pend:
        com,rot=r[4],r[7]
        for c in pend:
            d=[c[3][i]-com[i] for i in range(3)]
            cp.write('%.4f %.3f %.3f %.3f\n'%(t,sum(d[i]*rot[i] for i in range(3)),sum(d[i]*rot[3+i] for i in range(3)),sum(d[i]*rot[6+i] for i in range(3))))
        pend=None
P
rm -f $S/$tag/d$w.jsonl
