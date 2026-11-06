"""live_runs.py [box]: the Scout's runs on its tail in the live sidefight captures with the hits and fork strokes around them (docs/29 12bf). Run from the repository root."""
import json,sys,math,glob
sys.path.insert(0,'tools/oracle/sideways')
O='local/oracle/original/'
files=sorted(glob.glob(O+'sidefight-scout-alarm-octagon__live__0*.jsonl'))+sorted(glob.glob(O+'boxstart-actorder-scout-alarm-octagon__live__000.jsonl'))
if len(sys.argv)>1 and sys.argv[1]=='box': files=sorted(glob.glob(O+'sidefight-scout-alarm-box__live__0*.jsonl'))+sorted(glob.glob(O+'boxstart-actorder-scout-alarm-box__live__000.jsonl'))
tot=0
for f in files:
    t=0.0; S=[]; ev=[]  # ev: (t,kind,data)
    for l in open(f):
        if l.startswith('["A"'): t+=json.loads(l)[2]
        elif l.startswith('["S"'): S.append((t,json.loads(l)[2]))
        elif l.startswith('["FM"'): ev.append((t,'fork'))
        elif l.startswith('["H"'):
            r=json.loads(l); ev.append((t,'hit',r[2],r[4],r[6]))
    first=S[0][1]
    idx=min(range(len(first)),key=lambda i:(first[i][1]+5.96)**2+(first[i][3]-5.75)**2)
    bell=None
    for tt,ch in S:
        v=math.sqrt(sum(x*x for x in ch[idx][4:7]))
        if v>0.8 and ch[idx][2]<0.6 and bell is None: bell=tt-0.3
    runs=[];s0=None
    for tt,ch in S:
        if tt<bell: continue
        y=ch[idx][14]
        if abs(y)<0.3 and s0 is None: s0=tt
        if abs(y)>=0.3 and s0 is not None: runs.append((s0,tt)); s0=None
    print(f.split('/')[-1][:50],'bell %.1f'%bell,'runs',[(round(a-bell,1),round(b-a,1)) for a,b in runs if b-a>=0.3])
    for a,b in runs:
        if b-a<0.5: continue
        inr=[e for e in ev if a-0.3<=e[0]<=b+0.3]
        print('   run %.1f..%.1f (%.1fs):'%(a-bell,b-bell,b-a),[(round(e[0]-bell,1),)+e[1:] for e in inr][:14])
