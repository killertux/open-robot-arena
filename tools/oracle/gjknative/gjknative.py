#!/usr/bin/env python3
"""gjknative.py: drive `gjkn` (build.sh), the original's distance query and Johnson reduction run
natively (docs/29 section 12an).

    gjknative.py replay <cases.txt> [GJKN_DIR]
        the queries of tools/oracle/gjk_cases.py's file through the original's code under the x87
        control words 0x7f (24-bit), 0x27f (53-bit) and 0x37f (64-bit), against the live rows
    gjknative.py dump <cases.txt> <out.tsv> [cw]
        the native answers as a TSV for the Rust tests/examples (S rows: hulls, Q rows: queries)
    gjknative.py cxcx <n> <seed> <out.tsv> [cw]
        n random hull x hull queries through FUN_0055b2c0 (the convex x convex query, docs/29
        section 12ax): two hulls (boxes, prisms, clouds, a capsule's two points, a sphere's one),
        random poses from touching to far, with and without adjacency lists
    gjknative.py reduce <n> <seed> <out.tsv> [cw]
        n random / flat / collinear / duplicate-point simplexes and the native reduction's answer

The record layout (BLOB bytes at 0x30000000 in `gjkn`): the collidable at +0x100 (3x3 at +0x138,
translation at +0x15c, inverse 3x3 at +0x178, shape at +0x1bc, adjacency at +0x1d0), the hull's
16-byte vertices at +0x800, the adjacency entries (16 bytes: lists at +4 / +8) at +0xa40, the
triangle at +0x2000, the support objects at +0x3000 / +0x3100, the output at +0x3200 and the
simplex state at +0x3300.
"""
import math
import os
import random
import struct
import subprocess
import sys

BASE, BLOB = 0x30000000, 0x4000
O = dict(C=0x100, P1=0x400, S=0x700, VERTS=0x800, ADJ=0xA00, ENT=0xA40, LISTS=0xC40,
         TRI=0x2000, SA=0x3000, SB=0x3100, OUT=0x3200, ST=0x3300)
DIR = os.environ.get("GJKN_DIR", ".")


def build(verts, adj, xf, tri, start):
    b = bytearray(BLOB)
    w32 = lambda o, v: struct.pack_into("<I", b, o, v & 0xFFFFFFFF)
    wf = lambda o, v: struct.pack_into("<f", b, o, v)
    A = lambda k: BASE + O[k]
    C = O["C"]
    w32(C + 0x1C, A("P1")); w32(O["P1"] + 0x25C, 7); w32(C + 0x130, 7)
    m = xf[:9]
    for i, v in enumerate(m): wf(C + 0x138 + 4 * i, v)
    for i in range(3): wf(C + 0x138 + 0x24 + 4 * i, xf[9 + i])
    inv = [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]]  # the inverse is the transpose
    for i, v in enumerate(inv): wf(C + 0x178 + 4 * i, v)
    w32(C + 0x1BC, A("S")); w32(O["S"] + 0x1C, A("VERTS"))
    for i, (x, y, z) in enumerate(verts):
        o = O["VERTS"] + 16 * i; wf(o, x); wf(o + 4, y); wf(o + 8, z)
    if adj is not None:
        w32(C + 0x1D0, A("ADJ")); w32(O["ADJ"] + 4, A("ENT"))
        p = O["LISTS"]
        for i, l in enumerate(adj):
            w32(O["ENT"] + 16 * i + 4, BASE + p)
            for j, n in enumerate(l): struct.pack_into("<H", b, p + 2 * j, n)
            p += 2 * len(l); w32(O["ENT"] + 16 * i + 8, BASE + p)
        assert p < O["TRI"]
    for i in range(3):
        for k in range(3): wf(O["TRI"] + 16 * i + 4 * k, tri[3 * i + k])
    w32(O["SA"], A("C")); struct.pack_into("<H", b, O["SA"] + 4, start)
    w32(O["SB"], A("TRI"))
    return bytes(b)


def parse(out):
    b = out[:BLOB]; res = struct.unpack("<d", out[BLOB:BLOB + 8])[0]
    cnt = struct.unpack_from("<I", b, O["ST"] + 0x20)[0]
    ia = [(struct.unpack_from("<I", b, O["ST"] + 4 * i)[0] - BASE - O["VERTS"]) // 16 for i in range(cnt)]
    ib = [(struct.unpack_from("<I", b, O["ST"] + 16 + 4 * i)[0] - BASE - O["TRI"]) // 16 for i in range(cnt)]
    onA = list(struct.unpack_from("<3f", b, O["OUT"] + 0x20))
    dist = struct.unpack_from("<f", b, O["OUT"] + 0x44)[0]
    after = struct.unpack_from("<H", b, O["SA"] + 4)[0]
    return cnt, ia, ib, onA, dist, res, after


def run(blobs, cw, mode=0):
    out = b""
    for i in range(0, len(blobs), 4000):
        p = subprocess.Popen([os.path.join(DIR, "gjkn"), os.path.join(DIR, "img.bin")],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        o, _ = p.communicate(struct.pack("<II", cw, mode) + b"".join(blobs[i:i + 4000]))
        out += o
    rec = BLOB + 8
    assert len(out) == len(blobs) * rec
    return [out[i * rec:(i + 1) * rec] for i in range(len(blobs))]


def parse_cases(path):
    shapes, js = {}, []
    for line in open(path):
        t = line.split()
        if t[0] == "S":
            n = int(t[2]); f = lambda i: float(t[3 + i])
            verts = [(f(3 * i), f(3 * i + 1), f(3 * i + 2)) for i in range(n)]
            adj = None; rest = 3 + 3 * n
            if len(t) > rest and t[rest] == "adj":
                k = rest + 2; adj = []
                for _ in range(n):
                    l = int(t[k]); adj.append([int(x) for x in t[k + 1:k + 1 + l]]); k += 1 + l
            shapes[t[1]] = (verts, adj)
        elif t[0] == "J":
            cnt = int(t[4]); ia = [int(x) for x in t[5:5 + cnt]]
            k = 5 + 2 * cnt; dist = float(t[k]); k += 1
            nums = [float(x) for x in t[k:k + 24]]
            js.append(dict(shape=t[1], seq=int(t[2]), start=int(t[3]), ia=ia, dist=dist,
                           xf=nums[:12], tri=nums[12:21], onA=nums[21:24]))
    return shapes, js


def replay(path):
    shapes, js = parse_cases(path)
    blobs = [build(*shapes[j["shape"]], j["xf"], j["tri"], j["start"]) for j in js]
    for cw in (0x7F, 0x27F, 0x37F):
        n = same = pt = 0
        for j, o in zip(js, run(blobs, cw)):
            cnt, ia, ib, onA, dist, res, after = parse(o)
            n += 1
            ok = sorted(set(ia)) == sorted(set(j["ia"]))
            same += ok
            pt += ok and (res < 0 or math.dist(onA, j["onA"]) < 2e-3)
        print(f"cw {cw:#x}: {n} queries, same vertex set {same}, and hull point within 2 mm {pt}")


def dump(path, out, cw):
    shapes, js = parse_cases(path)
    names = {}
    with open(out, "w") as f:
        for k, (verts, adj) in shapes.items():
            names[k] = len(names)
            row = ["S", str(names[k]), str(len(verts))] + [repr(x) for v in verts for x in v]
            row += ["adj"] + [str(len(l)) for l in adj] + [str(i) for l in adj for i in l] if adj is not None else ["noadj"]
            f.write("\t".join(row) + "\n")
        blobs = [build(*shapes[j["shape"]], j["xf"], j["tri"], j["start"]) for j in js]
        for j, o in zip(js, run(blobs, cw)):
            cnt, ia, ib, onA, dist, res, after = parse(o)
            row = ["Q", str(names[j["shape"]]), str(j["start"])] + [repr(x) for x in j["xf"][:12]] + [repr(x) for x in j["tri"]]
            row += ["|", repr(res), str(cnt)] + [str(x) for x in ia] + ["/"] + [str(x) for x in ib] + ["/"]
            row += [repr(x) for x in onA] + [repr(dist), str(after)]
            f.write("\t".join(row) + "\n")


# The second hull of a hull x hull query (mode 2): its collidable, shape, vertices and adjacency.
O2 = dict(C=0x2400, P1=0x2700, S=0x2800, VERTS=0x2900, ADJ=0x2B00, ENT=0x2B40, LISTS=0x2D40)


def put_hull(b, o, verts, adj, xf, start, sa):
    """One hull's collidable (offsets `o`) and its support object `sa`."""
    w32 = lambda off, v: struct.pack_into("<I", b, off, v & 0xFFFFFFFF)
    wf = lambda off, v: struct.pack_into("<f", b, off, v)
    A = lambda k: BASE + o[k]
    C = o["C"]
    w32(C + 0x1C, A("P1")); w32(o["P1"] + 0x25C, 7); w32(C + 0x130, 7)
    for i, v in enumerate(xf[:9]): wf(C + 0x138 + 4 * i, v)
    for i in range(3): wf(C + 0x138 + 0x24 + 4 * i, xf[9 + i])
    m = xf[:9]
    for i, v in enumerate([m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]]): wf(C + 0x178 + 4 * i, v)
    w32(C + 0x1BC, A("S")); w32(o["S"] + 0x1C, A("VERTS")); w32(o["S"] + 0x18, len(verts))
    for i, (x, y, z) in enumerate(verts):
        q = o["VERTS"] + 16 * i; wf(q, x); wf(q + 4, y); wf(q + 8, z)
    if adj is not None:
        w32(C + 0x1D0, A("ADJ")); w32(o["ADJ"] + 4, A("ENT"))
        p = o["LISTS"]
        for i, l in enumerate(adj):
            w32(o["ENT"] + 16 * i + 4, BASE + p)
            for j, n in enumerate(l): struct.pack_into("<H", b, p + 2 * j, n)
            p += 2 * len(l); w32(o["ENT"] + 16 * i + 8, BASE + p)
    w32(sa, A("C")); struct.pack_into("<H", b, sa + 4, start)


def build_cc(h1, h2):
    b = bytearray(BLOB)
    put_hull(b, O, *h1, O["SA"])
    put_hull(b, O2, *h2, O["SB"])
    return bytes(b)


def parse_cc(out):
    b = out[:BLOB]; res = struct.unpack("<d", out[BLOB:BLOB + 8])[0]
    cnt = struct.unpack_from("<I", b, O["ST"] + 0x20)[0]
    ia = [(struct.unpack_from("<I", b, O["ST"] + 4 * i)[0] - BASE - O["VERTS"]) // 16 for i in range(cnt)]
    ib = [(struct.unpack_from("<I", b, O["ST"] + 16 + 4 * i)[0] - BASE - O2["VERTS"]) // 16 for i in range(cnt)]
    f = lambda off, n: list(struct.unpack_from("<%df" % n, b, O["OUT"] + off))
    return dict(res=res, cnt=cnt, ia=ia, ib=ib, normal=f(0, 3), point=f(0x10, 3), oa=f(0x20, 3), ob=f(0x30, 3),
                dist=f(0x44, 1)[0], after_a=struct.unpack_from("<H", b, O["SA"] + 4)[0],
                after_b=struct.unpack_from("<H", b, O["SB"] + 4)[0])


def rand_hull(rnd, r32):
    kind = rnd.choice(["box", "box", "prism", "cloud", "cloud", "capsule", "sphere"])
    if kind == "box":
        hx, hy, hz = [rnd.uniform(0.05, 0.8) for _ in range(3)]
        v = [(sx * hx, sy * hy, sz * hz) for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)]
    elif kind == "prism":
        n = rnd.choice([6, 8, 12]); rr = rnd.uniform(0.1, 0.6); hh = rnd.uniform(0.05, 0.5)
        v = [(rr * math.cos(2 * math.pi * k / n), y, rr * math.sin(2 * math.pi * k / n)) for k in range(n) for y in (-hh, hh)]
    elif kind == "cloud":
        v = [(rnd.uniform(-.6, .6), rnd.uniform(-.6, .6), rnd.uniform(-.6, .6)) for _ in range(rnd.randint(4, 14))]
    elif kind == "capsule":
        h = rnd.uniform(0.1, 0.6); v = [(0, -h, 0), (0, h, 0)]
    else:
        v = [(0.0, 0.0, 0.0)]
    v = [tuple(r32(c) for c in p) for p in v]
    adj = None
    if rnd.random() < 0.5 and 1 < len(v) <= 16:
        adj = [[j for j in range(len(v)) if j != i] for i in range(len(v))]
    return v, adj


def rand_pose(rnd, r32):
    # a random rotation from a unit quaternion
    q = [rnd.gauss(0, 1) for _ in range(4)]; n = math.sqrt(sum(x * x for x in q)); x, y, z, w = [c / n for c in q]
    m = [1 - 2 * (y * y + z * z), 2 * (x * y + z * w), 2 * (x * z - y * w),
         2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w),
         2 * (x * z + y * w), 2 * (y * z - x * w), 1 - 2 * (x * x + y * y)]
    return [r32(c) for c in m]


def cxcx_cases(n, seed, out, cw):
    rnd = random.Random(seed)
    r32 = lambda x: struct.unpack("<f", struct.pack("<f", x))[0]
    cases = []
    for _ in range(n):
        h1, h2 = rand_hull(rnd, r32), rand_hull(rnd, r32)
        reach = rnd.choice([0.3, 1.0, 2.0, 3.0])
        t1 = [r32(rnd.uniform(-reach, reach)) for _ in range(3)]
        t2 = [r32(rnd.uniform(-reach, reach)) for _ in range(3)]
        xf1 = rand_pose(rnd, r32) + t1
        xf2 = rand_pose(rnd, r32) + t2
        s1, s2 = rnd.randrange(len(h1[0])), rnd.randrange(len(h2[0]))
        cases.append(((h1[0], h1[1], xf1, s1), (h2[0], h2[1], xf2, s2)))
    outs = run([build_cc(*c) for c in cases], cw, 2)
    names = {}
    with open(out, "w") as f:
        for c, o in zip(cases, outs):
            p = parse_cc(o)
            row = ["C"]
            for verts, adj, xf, start in c:
                row += [str(len(verts))] + [repr(x) for v in verts for x in v] + ["full" if adj is not None else "noadj"]
                row += [repr(x) for x in xf] + [str(start)]
            row += ["|", repr(p["res"]), str(p["cnt"])] + [str(x) for x in p["ia"]] + ["/"] + [str(x) for x in p["ib"]] + ["/"]
            row += [repr(x) for x in p["normal"] + p["point"] + p["oa"] + p["ob"]] + [repr(p["dist"]), str(p["after_a"]), str(p["after_b"])]
            f.write("\t".join(row) + "\n")


def build_reduce(w):
    b = bytearray(BLOB)
    for i, (x, y, z) in enumerate(w): struct.pack_into("<4f", b, 0x100 + 16 * i, x, y, z, 0.0)
    for i in range(4):
        struct.pack_into("<I", b, 0x200 + 4 * i, 100 + i); struct.pack_into("<I", b, 0x210 + 4 * i, 200 + i)
    struct.pack_into("<I", b, 0x220, len(w))
    return bytes(b)


def reduce_cases(n, seed, out, cw):
    random.seed(seed)
    r32 = lambda x: struct.unpack("<f", struct.pack("<f", x))[0]
    cases = []
    for _ in range(n):
        k = random.choice([2, 3, 3, 4, 4, 4]); kind = random.random(); sc = random.choice([1.0, 4.0, 0.01, 100.0])
        rp = lambda: r32(random.uniform(-sc, sc)); pts = []
        if kind < 0.3:      # flat, with duplicates
            c = rp()
            for _ in range(k):
                pts.append(list(random.choice(pts)) if pts and random.random() < 0.25 else [rp(), c, rp()])
        elif kind < 0.45:   # collinear
            a = [rp() for _ in range(3)]; d = [rp() for _ in range(3)]
            for _ in range(k):
                s = random.uniform(-2, 2); pts.append([r32(a[i] + s * d[i]) for i in range(3)])
        elif kind < 0.6:    # nearly flat
            c = rp()
            for _ in range(k): pts.append([rp(), r32(c + random.uniform(-1, 1) * 1e-5 * sc), rp()])
        else:
            for _ in range(k): pts.append([rp(), rp(), rp()])
        cases.append(pts)
    outs = run([build_reduce(c) for c in cases], cw, 1)
    with open(out, "w") as f:
        for c, o in zip(cases, outs):
            b = o[:BLOB]; ret = int(struct.unpack("<d", o[BLOB:BLOB + 8])[0])
            cnt = struct.unpack_from("<I", b, 0x220)[0]
            A = list(struct.unpack_from("<4I", b, 0x200)); lam = list(struct.unpack_from("<4f", b, 0x300))
            row = [str(len(c))] + [repr(v) for p in c for v in p] + ["|", str(ret), str(cnt)] + [str(a - 100) for a in A] + [repr(x) for x in lam]
            f.write("\t".join(row) + "\n")


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "replay": replay(sys.argv[2])
    elif cmd == "dump": dump(sys.argv[2], sys.argv[3], int(sys.argv[4], 0) if len(sys.argv) > 4 else 0x27F)
    elif cmd == "cxcx": cxcx_cases(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], int(sys.argv[5], 0) if len(sys.argv) > 5 else 0x27F)
    elif cmd == "reduce": reduce_cases(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], int(sys.argv[5], 0) if len(sys.argv) > 5 else 0x27F)
    else: sys.exit(__doc__)
