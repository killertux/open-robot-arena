#!/usr/bin/env python3
"""collider_compare.py <colliders-*.jsonl> <colliderdump> <game> <bot>...: every collidable
of the robots of a live fight against the pieces our robot builder makes (docs/29 section 12at).

The live file is frida/ra2_robotcolliders.js' (C rows: one per collidable with its shape
vertices, the transform to its body at +0x60 and the body's centre of mass; K rows: which
bodies a component owns or hangs on).  Ours is `examples/colliderdump` (one JSON line per
piece of each part, in the part's frame).  Both sides are reduced to what a body frame does
not change: a piece's signature is its vertex count and the sorted pairwise distances of its
vertices, and two pieces of one body are as far apart as their centroids are.

Per robot (matched to a bot by the chassis' vertex count and size): the pieces of each live
body, which of ours they match, and the pieces only one side has.
"""
import collections
import itertools
import json
import subprocess
import sys


def load(path):
    return [json.loads(l) for l in open(path)]


def sig(points):
    d = sorted(round(sum((a - b) ** 2 for a, b in zip(p, q)) ** 0.5, 2) for p, q in itertools.combinations(points, 2))
    return (len(points), tuple(d))


def centroid(points):
    n = len(points)
    return [sum(p[i] for p in points) / n for i in range(3)]


def dist(a, b):
    return sum((x - y) ** 2 for x, y in zip(a, b)) ** 0.5


def body_points(r):
    raw = r[14]
    m = [raw[0x60 // 4 + i] for i in range(9)]
    t = [raw[0x84 // 4 + i] for i in range(3)]
    # row-vector convention: out[i] = m[i]*x + m[3+i]*y + m[6+i]*z + t[i] (checked on the chassis)
    return [[m[i] * v[0] + m[3 + i] * v[1] + m[6 + i] * v[2] + t[i] for i in range(3)] for v in r[7]]


def groups(rows):
    par = {}

    def f(x):
        par.setdefault(x, x)
        while par[x] != x:
            par[x] = par[par[x]]
            x = par[x]
        return x

    bodies = {r[4] for r in rows if r[0] == "C"}
    for r in rows:
        if r[0] != "K":
            continue
        bs = [w[1] for w in r[5] if w[1] in bodies] + [w[2] for w in r[5] if w[2] in bodies]
        for b in bs[1:]:
            par[f(b)] = f(bs[0])
        for b in bs:
            f(b)
    return {b: f(b) for b in par}


def tri_set(points, tris, round_to=3):
    """The triangles of a mesh by their edge lengths (a frame, an index order or a vertex order
    does not change them): a sorted list of sorted edge-length triples."""
    def d(i, j):
        return round(sum((a - b) ** 2 for a, b in zip(points[i], points[j])) ** 0.5, round_to - 1)

    return sorted(tuple(sorted((d(t[0], t[1]), d(t[1], t[2]), d(t[0], t[2])))) for t in tris)


def ours(dump, game, bot):
    out = subprocess.run([dump, f"{game}/{bot}"], capture_output=True, text=True, check=True).stdout
    return [json.loads(l) for l in out.splitlines()]


def main():
    pairs = "--pairs" in sys.argv
    argv = [a for a in sys.argv if a != "--pairs"]
    path, dump, game, bots = argv[1], argv[2], argv[3], argv[4:]
    rows = load(path)
    # Every collidable seen in any pass: parts that came off during the capture are still there.
    C = list({r[1] + str(r[3]): r for r in rows if r[0] == "C"}.values())
    g = groups(rows)
    robots = collections.defaultdict(lambda: collections.defaultdict(list))
    for r in C:
        if r[4] in g:
            robots[g[r[4]]][r[4]].append(r)
    mine = {b: ours(dump, game, b) for b in bots}
    names = {}  # live tag -> "robot:piece"
    taken = set()
    for gid, bodies in robots.items():
        chassis = [r for bs in bodies.values() for r in bs if r[2] == 4 and r[5] >= 20]
        if not chassis:
            continue
        nv = chassis[0][5]
        bot = next((b for b in bots if b not in taken and any(p["tag"] == 0 and p["points"] == nv for p in mine[b])), None)
        if bot is None:
            print(f"robot with chassis of {nv} vertices: no bot left")
            continue
        taken.add(bot)
        label = bot.split("/")[-1][:-4]
        theirs = mine[bot]
        used = set()
        print(f"== {bot}: live {sum(len(v) for v in bodies.values())} collidables on {len(bodies)} bodies, ours {len(theirs)} pieces on {len({p['part'] for p in theirs})} parts")
        for b, cs in bodies.items():
            print(f" live body {b}")
            for r in sorted(cs, key=lambda r: r[3]):
                s = sig(body_points(r))
                kind = "mesh" if r[2] == 4 else "hull"
                hit = next((i for i, p in enumerate(theirs) if i not in used and sig(p["raw"]) == s), None)
                if hit is None:
                    pts = body_points(r)
                    dims = "x".join(f"{max(q[i] for q in pts) - min(q[i] for q in pts):.2f}" for i in range(3))
                    names[r[3]] = f"{label}:LIVE-ONLY(nv{r[5]}/nt{r[6]} {dims})"
                    print(f"   tag {r[3]:3d} {kind} nv {r[5]:2d} nt {r[6]:2d}: NOT IN OURS")
                else:
                    used.add(hit)
                    p = theirs[hit]
                    names[r[3]] = f"{label}:{p['name']}"
                    if r[2] == 4:
                        a = tri_set(r[7], r[8])
                        b = tri_set(p["raw"], p["tri_idx"])
                        print(f"        triangles: live {len(a)}, ours {len(b)}, {'identical' if a == b else 'DIFFER'}")
                    print(f"   tag {r[3]:3d} {kind} nv {r[5]:2d} nt {r[6]:2d}: ours part {p['part']} {p['name']} (comp {p['tag']}, {'mesh' if p['tris'] else 'hull'})")
        for i, p in enumerate(theirs):
            if i not in used:
                print(f"   ours only: part {p['part']} comp {p['tag']} {p['name']} pts {p['points']}")
    if pairs:
        print("== contact pairs of the capture (resolver, by collidable tag): n, min distance, below 0")
        for r in sorted((r for r in rows if r[0] == "RP"), key=lambda r: -r[4]):
            a, b = names.get(r[2], f"tag{r[2]}"), names.get(r[3], f"tag{r[3]}")
            print(f"  {a:40s} {b:40s} n {r[4]:6d} min {r[5]:.3f} neg {r[6]}")


if __name__ == "__main__":
    main()
