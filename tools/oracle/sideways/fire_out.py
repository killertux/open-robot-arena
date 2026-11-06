"""fire_out.py <tipab capture.jsonl|ours_tipab out>...: outcome (tail / wheels / back) of every trial of a tipab grid from the Scout chassis' B rows (docs/29 12bf)."""
import json,sys
def outcomes(path):
    L=[json.loads(l) for l in open(path)]
    res=[];cur=None
    lab=None
    for r in L:
        if r[0]=='TRIAL':
            if cur is not None and lab is not None: res.append((lab,cur))   # ours_tipab.py files have no `done` rows
            lab=r[2]['label']; cur=[]
        elif r[0]=='B' and cur is not None and abs(r[3]-163.49)<0.1 and len(r[7])==9: cur.append(r[7][4])
        elif r[0]=='X' and r[2]=='done' and cur is not None:
            res.append((lab,cur)); cur=None; lab=None
    if cur is not None and lab is not None: res.append((lab,cur))
    return res
for p in sys.argv[1:]:
    rs=outcomes(p)
    kinds=[]
    for lab,c in rs:
        y=c[-1] if c else 9
        k='wheels' if y>0.9 else ('back' if y<-0.9 else 'tail')
        kinds.append(k)
    n=len(kinds); print(p.split('/')[-1][:50],n,{k:kinds.count(k) for k in set(kinds)})
    print('  ',[ (l.replace('pitch ','p').replace(' fire ','f'),k[0]) for (l,_),k in zip(rs,kinds)])
