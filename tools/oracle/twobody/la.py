# tiny 3-vector / 3x3 algebra (no numpy here)
import math
def array(x, float=None):
    if x and isinstance(x[0], (list, tuple)): return [list(map(float_, r)) for r in x]
    return [float_(v) for v in x]
float_ = float
def zeros(n): return [0.0] * n
def add(a, b): return [x + y for x, y in zip(a, b)]
def sub(a, b): return [x - y for x, y in zip(a, b)]
def mul(a, s): return [x * s for x in a]
def dot(a, b): return sum(x * y for x, y in zip(a, b))
def cross(a, b): return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
def norm(a): return math.sqrt(dot(a, a))
def mv(M, v): return [dot(r, v) for r in M]
def mm(A, B): return [[sum(A[i][k]*B[k][j] for k in range(3)) for j in range(3)] for i in range(3)]
def T(A): return [[A[j][i] for j in range(3)] for i in range(3)]
def madd(A, B): return [[A[i][j]+B[i][j] for j in range(3)] for i in range(3)]
def msub(A, B): return [[A[i][j]-B[i][j] for j in range(3)] for i in range(3)]
def eye(s=1.0): return [[s if i == j else 0.0 for j in range(3)] for i in range(3)]
def det(A):
    return (A[0][0]*(A[1][1]*A[2][2]-A[1][2]*A[2][1]) - A[0][1]*(A[1][0]*A[2][2]-A[1][2]*A[2][0]) + A[0][2]*(A[1][0]*A[2][1]-A[1][1]*A[2][0]))
def inv(A):
    d = det(A)
    c = [[A[1][1]*A[2][2]-A[1][2]*A[2][1], A[0][2]*A[2][1]-A[0][1]*A[2][2], A[0][1]*A[1][2]-A[0][2]*A[1][1]],
         [A[1][2]*A[2][0]-A[1][0]*A[2][2], A[0][0]*A[2][2]-A[0][2]*A[2][0], A[0][2]*A[1][0]-A[0][0]*A[1][2]],
         [A[1][0]*A[2][1]-A[1][1]*A[2][0], A[0][1]*A[2][0]-A[0][0]*A[2][1], A[0][0]*A[1][1]-A[0][1]*A[1][0]]]
    return [[x / d for x in r] for r in c]
def skew(r): return [[0, -r[2], r[1]], [r[2], 0, -r[0]], [-r[1], r[0], 0]]
def pct(xs, q):
    xs = sorted(xs); 
    if not xs: return float('nan')
    return xs[min(len(xs)-1, int(q/100*len(xs)))]
