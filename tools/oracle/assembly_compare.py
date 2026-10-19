#!/usr/bin/env python3
"""Compare our robot assembly with the original's live scene.

    assembly_compare.py local/oracle/original/assembly-scout__live__000.json ours.json

`ours.json` is `cargo run --release -p ra2sim --example asmdump -- <bot>`.  Every
original "<object> Instance" node is expressed in the chassis body's frame (the
"Chassis Display" node) and matched with our object of the same name nearest to
it; prints both positions, the position error and the angle between the two
rotations.  `--first` compares the scene as built (the robot's first physics
step) instead of at rest.  `--json` prints the matched rows as JSON instead.
"""
import json
import math
import sys



class M4:
    """A 4x4 column-vector matrix (rows of lists)."""

    def __init__(self, r):
        self.r = r

    def __matmul__(self, o):
        return M4([[sum(self.r[i][k] * o.r[k][j] for k in range(4)) for j in range(4)] for i in range(4)])

    @property
    def t(self):
        return [self.r[0][3], self.r[1][3], self.r[2][3]]

    def rot(self):
        return [row[:3] for row in self.r[:3]]

    def inv_rigid(self):
        # Rotation part may carry scale-free orthonormal axes; invert as rigid.
        R = self.rot()
        Rt = [[R[j][i] for j in range(3)] for i in range(3)]
        t = self.t
        ti = [-sum(Rt[i][k] * t[k] for k in range(3)) for i in range(3)]
        return M4([Rt[0] + [ti[0]], Rt[1] + [ti[1]], Rt[2] + [ti[2]], [0, 0, 0, 1]])


def dist(a, b):
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


def rowmajor(m):
    # The game's matrices are D3D row-vector (p' = p M); return the
    # column-vector equivalent.
    return M4([[m[4 * j + i] for j in range(4)] for i in range(4)])


def walk(n, out, parent=None):
    out.append((n, parent))
    for k in n.get("kids", []):
        walk(k, out, n)


def live_instances(cap, which="tree"):
    nodes = []
    tree = cap["tree"] if which == "tree" else cap[which]["tree"]
    walk(tree, nodes)
    chassis = [n for n, _ in nodes if n.get("name") == "Chassis Display"]
    if not chassis:
        raise SystemExit("no Chassis Display node")
    inv = rowmajor(chassis[0]["world"]).inv_rigid()
    rows = []
    for n, parent in nodes:
        name = n.get("name", "")
        if not name.endswith(" Instance"):
            continue
        rows.append({"name": name[: -len(" Instance")].lower(), "m": inv @ rowmajor(n["world"]),
                     "body": parent.get("name") if parent else None})
    return rows


def ours_objects(path):
    rows = []
    for c in json.load(open(path)):
        for o in c["objects"]:
            # The original draws a component's own mesh at the component's
            # frame and a body of its own (rotor, steering hub) at its TM.
            m = o["m"] if (o.get("rotor") or o.get("host") or "frame" not in o) else o["frame"]
            rows.append({"name": o["name"], "m": M4([[m[4 * j + i] for j in range(4)] for i in range(4)]), "index": c["index"],
                         "class": c["class"], "part": c["part"]})
    return rows


def rot_angle(a, b):
    A, B = a.rot(), b.rot()
    tr = sum(A[i][k] * B[i][k] for i in range(3) for k in range(3))
    return math.degrees(math.acos(max(-1.0, min(1.0, (tr - 1) / 2))))


def match(live, ours):
    used = set()
    out = []
    for l in live:
        cands = [(dist(o["m"].t, l["m"].t), k) for k, o in enumerate(ours)
                 if o["name"] == l["name"] and k not in used]
        if not cands:
            out.append((l, None, None))
            continue
        d, k = min(cands)
        used.add(k)
        out.append((l, ours[k], d))
    return out


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    cap = json.load(open(args[0]))
    rows = match(live_instances(cap, "first" if "--first" in sys.argv else "tree"), ours_objects(args[1]))
    if "--json" in sys.argv:
        print(json.dumps([{"name": l["name"], "live": l["m"].t,
                           "ours": o["m"].t if o else None, "err": d} for l, o, d in rows]))
        return
    errs = []
    for l, o, d in rows:
        lp = l["m"].t
        if o is None:
            print(f"{l['name']:22s} live ({lp[0]:7.3f} {lp[1]:7.3f} {lp[2]:7.3f})  ours: none   [{l['body']}]")
            continue
        op = o["m"].t
        errs.append(d)
        print(f"{l['name']:22s} live ({lp[0]:7.3f} {lp[1]:7.3f} {lp[2]:7.3f})  ours ({op[0]:7.3f} {op[1]:7.3f} {op[2]:7.3f})"
              f"  err {d:6.3f} m  rot {rot_angle(l['m'], o['m']):6.1f} deg  [{l['body']}] c{o['index']} {o['class']}")
    if errs:
        print(f"matched {len(errs)}  mean {sum(errs) / len(errs):.3f} m  max {max(errs):.3f} m")


if __name__ == "__main__":
    main()
