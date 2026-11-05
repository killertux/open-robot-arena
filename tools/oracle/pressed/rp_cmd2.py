import sys; sys.path.insert(0,'.')
import ast,pickle,math,bisect,re,glob
def live_cmds(tsv,steps):
    rows=[]
    for l in open(tsv).read().split('\n')[1:]:
        if l.strip():
            t,r=l.split('\t'); rows.append((float(t),ast.literal_eval(r)))
    T=[x[0] for x in steps]
    best=None
    for off in [i*0.1 for i in range(0,1500)]:
        err=0;n=0
        for t,r in rows[::12]:
            i=bisect.bisect_left(T,t+off)
            if i>=len(T): break
            x=steps[i][1]; loc=r[0][1]
            err+=math.dist([x[0],x[2]],[loc[0],loc[2]]); n+=1
        if n>50 and (best is None or err/n<best[0]): best=(err/n,off)
    off=best[1]
    out={0:[],1:[]}
    for t,r in rows:
        for pl in (0,1): out[pl].append((t+off,float(r[pl][4]),float(r[pl][5])))
    return out,best
P=re.compile(r"ai t=([\d.]+) bot=(\d) \(([-\d.]+),([-\d.]+)\).*? throttle=(\S+) turn=(\S+) ")
def ours_cmds(log):
    out={0:[],1:[]}
    for l in open(log):
        m=P.match(l)
        if m: out[int(m.group(2))].append((float(m.group(1))+1.95,float(m.group(5)),float(m.group(6))))
    return out
def at(c,t):
    i=bisect.bisect_right([x[0] for x in c],t)-1
    return (c[i][1],c[i][2]) if i>=0 else (0,0)
if __name__=='__main__':
    for i in range(4):
        steps=pickle.load(open('live2s_%d.pkl'%i,'rb'))
        c,b=live_cmds('rp-out2/boxstart-sentinel-wideload-compressor__live__00%d.tsv'%i,steps); print(i,b)
