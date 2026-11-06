"""launch_stats.py [ensemble dirs]: the Scout's minimum up.y within 8 s of the bell, live against ours (docs/29 12bf). Run from the repository root."""
import json,glob,math,ast,sys,bisect
O='local/oracle/original/'
def live(files):
    r=[]
    for f in files:
        t=0.0;S=[]
        for l in open(f):
            if l.startswith('["A"'): t+=json.loads(l)[2]
            elif l.startswith('["S"'): S.append((t,json.loads(l)[2]))
        first=S[0][1]
        idx=min(range(len(first)),key=lambda i:(first[i][1]+5.96)**2+(first[i][3]-5.75)**2)
        bell=None
        for tt,ch in S:
            v=math.sqrt(sum(x*x for x in ch[idx][4:7]))
            if v>0.8 and ch[idx][2]<0.6: bell=tt-0.3;break
        r.append([(tt-bell,ch[idx][14],ch[idx][1],ch[idx][3]) for tt,ch in S if bell<=tt<=bell+8])
    return r
def ours(d):
    r=[]
    for f in sorted(glob.glob(d+'/t*.tsv')):
        rows=[]
        for line in open(f).read().split('\n')[1:]:
            if line.strip():
                t,x=line.split('\t'); a,_=ast.literal_eval(x); rows.append((float(t),a[-1],a[1][0],a[1][2],a[3]))
        # bell: first time the Scout is 0.3 m from start
        sx,sz=rows[0][2],rows[0][3]
        b=next((t for t,y,x,z,v in rows if math.hypot(x-sx,z-sz)>0.3),None)
        r.append([(t-b,y,x,z) for t,y,x,z,v in rows if b<=t<=b+8])
    return r
def summ(name,runs):
    mins=[min(y for t,y,*_ in r) for r in runs]
    n=len(mins)
    print('%-8s n=%3d  min up.y within 8 s of the bell: <0.9 %2.0f%%  <0.7 %2.0f%%  <0.5 %2.0f%%  <0.3 %2.0f%%  <0 %2.0f%%   in first 3 s: <0.7 %2.0f%% <0.3 %2.0f%%'%(name,n,*[100*sum(m<th for m in mins)/n for th in (0.9,0.7,0.5,0.3,0.0)],*[100*sum(min([y for t,y,*_ in r if t<=3] or [1])<th for r in runs)/n for th in (0.7,0.3)]))
for arena in ('octagon','box'):
    L=sorted(glob.glob(O+'sidefight-scout-alarm-%s__live__0*.jsonl'%arena))+[O+'boxstart-actorder-scout-alarm-%s__live__000.jsonl'%arena]
    print(arena); summ('live',live(L))
    for m in sys.argv[1:]: summ(m.split('/')[-1],ours(m+'/'+arena))
