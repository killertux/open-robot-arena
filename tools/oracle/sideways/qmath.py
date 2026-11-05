import math
def qmul(a,b):
    w1,x1,y1,z1=a;w2,x2,y2,z2=b
    return (w1*w2-x1*x2-y1*y2-z1*z2,w1*x2+x1*w2+y1*z2-z1*y2,w1*y2-x1*z2+y1*w2+z1*x2,w1*z2+x1*y2-y1*x2+z1*w2)
def qconj(a):return (a[0],-a[1],-a[2],-a[3])
def qaxis(axis,deg):
    n=math.sqrt(sum(c*c for c in axis));h=math.radians(deg)/2
    return (math.cos(h),)+tuple(math.sin(h)*c/n for c in axis)
def qrot(q,v):
    p=(0,)+tuple(v);r=qmul(qmul(q,p),qconj(q));return r[1:]
def to_axis_angle(q):
    w=max(-1,min(1,q[0]));ang=2*math.acos(w);s=math.sqrt(1-w*w)
    if s<1e-9:return (1,0,0),0
    return tuple(c/s for c in q[1:]),math.degrees(ang)
def from_rows(m): # m: list of 3 rows = local axes in world -> R columns=axes
    R=[[m[0][0],m[1][0],m[2][0]],[m[0][1],m[1][1],m[2][1]],[m[0][2],m[1][2],m[2][2]]]
    t=R[0][0]+R[1][1]+R[2][2]
    if t>0:
        s=math.sqrt(t+1)*2;w=.25*s;x=(R[2][1]-R[1][2])/s;y=(R[0][2]-R[2][0])/s;z=(R[1][0]-R[0][1])/s
    elif R[0][0]>R[1][1] and R[0][0]>R[2][2]:
        s=math.sqrt(1+R[0][0]-R[1][1]-R[2][2])*2;w=(R[2][1]-R[1][2])/s;x=.25*s;y=(R[0][1]+R[1][0])/s;z=(R[0][2]+R[2][0])/s
    elif R[1][1]>R[2][2]:
        s=math.sqrt(1+R[1][1]-R[0][0]-R[2][2])*2;w=(R[0][2]-R[2][0])/s;x=(R[0][1]+R[1][0])/s;y=.25*s;z=(R[1][2]+R[2][1])/s
    else:
        s=math.sqrt(1+R[2][2]-R[0][0]-R[1][1])*2;w=(R[1][0]-R[0][1])/s;x=(R[0][2]+R[2][0])/s;y=(R[1][2]+R[2][1])/s;z=.25*s
    n=math.sqrt(w*w+x*x+y*y+z*z);return (w/n,x/n,y/n,z/n)
