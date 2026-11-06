#!/usr/bin/env python3
"""live_state.py capture.jsonl place_row_index out.txt: the state file of `tipprobe live` (docs/29 12be) from the `place` X row at that line of a tipab capture:
one line per body: mass, centre, velocity, angular velocity, rotation (columns), the original's local inertia (P row)."""
import json,sys
path,idx,out=sys.argv[1],int(sys.argv[2]),sys.argv[3]
L=[json.loads(l) for l in open(path)]
x=L[idx]; assert x[0]=='X' and x[2]=='place'
bodies=list(x[3])
j=idx+1
while L[j][0]=='X' and L[j][2]=='place':   # an `extra` group is a second X row
    bodies+=L[j][3]; j+=1
mass={}
inert={}
for r in L[:idx]:
    if r[0]=='P':
        f=r[3]
        inert[r[2]]=[f[0x19c//4+4*i+j] for i in range(3) for j in range(3)]
for r in L[idx:idx+400]:
    if r[0]=='B': mass[r[2]]=r[3]
for r in L[:idx]:
    if r[0]=='P': mass[r[2]]=r[3][0x190//4]
with open(out,'w') as f:
    for b in bodies:
        ptr,com,q,v,w=b
        W,X,Y,Z=q
        # rotation matrix rows
        R=[[1-2*(Y*Y+Z*Z),2*(X*Y-Z*W),2*(X*Z+Y*W)],[2*(X*Y+Z*W),1-2*(X*X+Z*Z),2*(Y*Z-X*W)],[2*(X*Z-Y*W),2*(Y*Z+X*W),1-2*(X*X+Y*Y)]]
        cols=[R[r][c] for c in range(3) for r in range(3)]
        f.write(' '.join(str(t) for t in [mass[ptr]]+com+v+w+cols+inert.get(ptr,[]))+'\n')
