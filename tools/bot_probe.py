#!/usr/bin/env python3
"""Robot Arena 2 `.bot` parser probe (line-based text + binary TGA blobs).

Validated against all shipped bot files. Used to derive/verify the Rust
implementation in engine/crates/ra2fmt::bot.
"""
from __future__ import annotations

import glob
import struct
import sys


class Cur:
    def __init__(self, d: bytes):
        self.d = d
        self.p = 0

    def line(self) -> bytes:
        j = self.d.find(b"\n", self.p)
        if j < 0:
            j = len(self.d)
        s = self.d[self.p:j]
        self.p = j + 1
        return s

    def ints(self, n=None):
        parts = self.line().split()
        vals = [int(x) for x in parts]
        return vals[:n] if n else vals

    def floats(self, n=None):
        parts = self.line().split()
        vals = [float(x) for x in parts]
        return vals[:n] if n else vals

    def skip_tga(self, expect=None):
        """Skip an uncompressed TGA using its own header."""
        h = self.d[self.p:self.p + 18]
        idlen = h[0]
        itype = h[2]
        w = struct.unpack_from("<H", h, 12)[0]
        hgt = struct.unpack_from("<H", h, 14)[0]
        bpp = h[16]
        if itype not in (2, 3):
            raise ValueError(f"TGA type {itype} not plain")
        if expect and (w, hgt) != expect:
            raise ValueError(f"TGA size {w}x{hgt} != {expect}")
        self.p += 18 + idlen + w * hgt * (bpp // 8)


def parse_3d_data(cur: Cur):
    """Return list of models: dict(points, indices)."""
    head = cur.line().split()      # b'274' totalpts totalidx totalfaces
    nmodel = int(cur.line())
    models = []
    for _ in range(nmodel):
        parts = cur.line().split()  # indexcount facecount pointcount 0
        idxc, facec, ptc = int(parts[0]), int(parts[1]), int(parts[2])
        assert cur.line().strip() == b"RAW", "expected RAW"
        normals = [cur.line() for _ in range(ptc)]
        uvs = [cur.line() for _ in range(ptc)]
        pts = [tuple(map(float, cur.line().split())) for _ in range(ptc)]
        idx = [int(cur.line()) for _ in range(idxc)]
        models.append(dict(pts=pts, idx=idx, uv=uvs, normals=normals))
    return models


def parse_chassis(cur: Cur, version=(1, 12)):
    bpc = int(cur.line())
    bottom = [cur.ints() for _ in range(bpc)]
    tpc = int(cur.line())
    top = [cur.ints() for _ in range(tpc)]
    hf = cur.line().split()
    height, nfaces = float(hf[0]), int(hf[1])

    models = None
    has_model = cur.line().strip() == b"true"
    if has_model:
        models = parse_3d_data(cur)
        has_tex = cur.line().strip() == b"true"
        if has_tex:
            cur.skip_tga((256, 256))
        if version > (1, 7):
            unknown = cur.line().strip()
            if unknown == b"true":
                raise ValueError("unknown chassis section present")
        # face groups
        ng = int(cur.line())
        for _ in range(ng):
            npts = int(cur.line())
            for _ in range(npts):
                cur.line()
        ng2 = int(cur.line())
        for _ in range(ng2):
            cur.line()
        nfe = int(cur.line())
        for _ in range(nfe):
            cur.line()
        nce = int(cur.line())
        for _ in range(nce):
            cur.line()
    # armour: type row, plus an extra row when a 3D model is present
    cur.line()                     # armour row
    if has_model:
        cur.line()                 # unknown/armour row
    # buckling points (1.11+; absent in 1.08)
    if version > (1, 8):
        bpn = int(cur.line())
        for _ in range(bpn):
            cur.line()             # x y z
            ic = int(cur.line())
            for _ in range(ic):
                cur.line()
    return dict(bottom=bottom, top=top, height=height, faces=nfaces, models=models)


def parse_bot(path: str):
    cur = Cur(open(path, "rb").read())
    ver = cur.line().decode("latin1")
    name = cur.line().decode("latin1")
    cls = cur.line().decode("latin1")
    try:
        major, minor = (int(x) for x in ver.split("."))
    except ValueError:
        major, minor = (1, 12)
    version = (major, minor)
    # The "default texture" int line exists only in newer botfiles; older ones
    # (1.07/1.08) put the snapshot flag here directly.
    x = cur.line().strip()
    if x in (b"true", b"false"):
        snap = x == b"true"
    else:
        # `x` was the default-texture flag line; snapshot flag follows.
        snap = cur.line().strip() == b"true"
    if snap:
        cur.skip_tga((128, 128))
    n = int(cur.line())
    comps = []
    for _ in range(n):
        parent = int(cur.line())
        mpc = int(cur.line())
        mp = int(cur.line())
        tr = cur.floats()
        rot = cur.floats()
        ah = cur.floats()
        cclass = cur.line().decode("latin1")
        cfile = cur.line().decode("latin1")
        extra = None
        if cclass == "Chassis":
            extra = parse_chassis(cur, version)
        elif cclass == "BurstMotor":
            extra = cur.floats()
        elif cclass == "SmartZone":
            cur.line()
        comps.append(dict(parent=parent, mpc=mpc, mp=mp, tr=tr, rot=rot, ah=ah,
                          cls=cclass, file=cfile, extra=extra))
    # buckle model + heading (1.11+; absent in 1.07/1.08)
    heading = 0.0
    if version > (1, 8):
        if cur.line().strip() == b"true":
            parse_3d_data(cur)
        heading = float(cur.line())
    # controllers
    nctrl = int(cur.line())
    for _ in range(nctrl):
        cur.line()                              # Name: ...
        cur.line()                              # x y type
        nb = int(cur.line())
        for _ in range(nb):
            cur.line()
    # bindings
    nbind = int(cur.line())
    for _ in range(nbind):
        cur.line()
        cur.line()
        cur.line()
    consumed = cur.p / len(cur.d)
    return dict(ver=ver, name=name, cls=cls, n=n, comps=comps,
                heading=heading, consumed=consumed)


def main():
    paths = sorted(glob.glob("**/*.bot", recursive=True))
    ok, bad = 0, []
    for p in paths:
        try:
            r = parse_bot(p)
            ok += 1
            if r["consumed"] < 0.995:
                bad.append((p, f"only {r['consumed']*100:.1f}% consumed"))
            elif "Bot0" in p and p.startswith("AI/Team0"):
                ch = next(c for c in r["comps"] if c["cls"] == "Chassis")
                m = ch["extra"]["models"][0]
                print(f"{p}: {r['name']} v{r['ver']} comps={r['n']} "
                      f"chassis pts={len(m['pts'])} idx={len(m['idx'])} "
                      f"height={ch['extra']['height']:.3f} consumed={r['consumed']*100:.1f}%")
        except Exception as e:  # noqa: BLE001
            bad.append((p, f"{type(e).__name__}: {e}"))
    print(f"\nparsed ok={ok}/{len(paths)}")
    for p, e in bad[:15]:
        print("  BAD", p, "->", e)


if __name__ == "__main__":
    main()
