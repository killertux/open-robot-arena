#!/usr/bin/env python3
"""GMI / .gmf parser and exporter for Robot Arena 2.

`.gmf` is a binary form of the 3ds Max `.ase` export plus RA2 attachment points
and physics metadata (rigid bodies, constraints, dashpots). Node/chunk structure
and field order are derived from the community PyRA2 project (Martijn Pieters,
LGPL); this is an independent Python 3 reimplementation.

Usage:
    python3 tools/gmi.py info  <file.gmf> [...]
    python3 tools/gmi.py obj   <file.gmf> <out.obj> [--scale S]
"""
from __future__ import annotations

import argparse
import struct
import sys
from dataclasses import dataclass, field
from typing import Optional


class Reader:
    def __init__(self, data: bytes):
        self.d = data
        self.p = 0

    def read(self, n: int) -> bytes:
        b = self.d[self.p:self.p + n]
        if len(b) != n:
            raise EOFError(f"wanted {n} bytes at {self.p}, got {len(b)}")
        self.p += n
        return b

    def i32(self) -> int:
        return struct.unpack("<i", self.read(4))[0]

    def f32(self) -> float:
        return struct.unpack("<f", self.read(4))[0]

    def u8(self) -> int:
        return self.read(1)[0]

    def s(self) -> str:
        n = self.i32()
        if n <= 0:
            return ""
        v = self.read(n)
        if v.endswith(b"\x00"):
            v = v[:-1]
        return v.decode("latin1")

    def color(self):
        r, g, b, _ = struct.unpack("<BBBB", self.read(4))
        return (r / 255.0, g / 255.0, b / 255.0)

    def vec3(self):
        return struct.unpack("<fff", self.read(12))

    def triplet(self):
        return struct.unpack("<iii", self.read(12))

    def tm(self):
        return tuple(self.vec3() for _ in range(4))

    def node(self):
        return self.triplet()  # (id, flags, length)


@dataclass
class Mesh:
    time: int = 0
    material_ref: int = 0
    points: list = field(default_factory=list)
    faces: list = field(default_factory=list)
    texcoords: list = field(default_factory=list)
    texfaces: list = field(default_factory=list)
    normals: list = field(default_factory=list)
    backface_cull: int = 1


@dataclass
class Material:
    name: str = ""
    klass: str = ""
    diffuse: tuple = (0, 0, 0)
    textures: list = field(default_factory=list)   # {"name","bitmap","klass"}


@dataclass
class Element:
    kind: str
    name: str = ""
    parent: str = ""
    tm: tuple = ()
    mesh: Optional[Mesh] = None
    data: str = ""
    extra: dict = field(default_factory=dict)


@dataclass
class Scene:
    materials: list = field(default_factory=list)
    objects: list = field(default_factory=list)

    @property
    def meshes(self):
        return [o for o in self.objects if o.mesh is not None]

    @property
    def attachments(self):
        return [o for o in self.objects if o.kind == "attachment"]


class GMIParser(Reader):
    """Parses the documented skeleton. Nodes whose length is untrustworthy
    (documented in PyRA2) are read structurally rather than by seeking."""

    def parse(self) -> Scene:
        if self.read(3) != b"GMI":
            raise ValueError("not a GMI file")
        if (self.i32(), self.i32(), self.i32(), self.i32()) != (3, 1, 0, 15):
            raise ValueError("unexpected GMI header")
        scene = Scene()
        self._read_scene()
        scene.materials = self._read_material_list()
        self._read_object_list(scene)
        return scene

    # -- shared -----------------------------------------------------------
    def _tm_node(self) -> tuple:
        nid, flags, _ = self.node()
        if (nid, flags) != (17, 2):
            raise ValueError(f"expected TM node, got {nid}/{flags}")
        self.s()              # node name
        return self.tm()

    # -- header -----------------------------------------------------------
    def _read_scene(self):
        nid, flags, _ = self.node()
        if (nid, flags) != (1, 2):
            raise ValueError(f"expected Scene node, got {nid}/{flags}")
        self.s()              # source
        self.i32(); self.i32(); self.i32(); self.i32()
        self.color(); self.color()

    def _read_material_list(self, sizehint: Optional[int] = None) -> list:
        nid, flags, _ = self.node()
        if (nid, flags) != (7, 2):
            raise ValueError(f"expected MaterialList, got {nid}/{flags}")
        count = self.i32()
        if sizehint is not None and count != sizehint:
            raise ValueError("material count mismatch")
        return [self._read_material() for _ in range(count)]

    def _read_material(self) -> Material:
        nid, flags, _ = self.node()
        if (nid, flags) != (8, 2):
            raise ValueError(f"expected Material, got {nid}/{flags}")
        self.i32()            # ref no
        m = Material()
        m.name = self.s()
        m.klass = self.s()
        self.color()          # ambient
        m.diffuse = self.color()
        self.color()          # specular
        self.f32(); self.f32(); self.f32(); self.f32()   # shine, shine_str, trans, wire
        self.i32()            # shading
        self.f32(); self.f32()                            # xp_falloff, self_illum
        self.i32()            # two_sided
        self.i32()            # xp_type
        texcount = self.i32()
        if texcount:
            m.textures = self._read_texture_list(texcount)
        subcount = self.i32()
        if subcount:
            self._read_material_list(subcount)
        return m

    def _read_texture_list(self, sizehint: Optional[int] = None) -> list:
        nid, flags, _ = self.node()
        if (nid, flags) != (14, 2):
            raise ValueError(f"expected TextureList, got {nid}/{flags}")
        count = self.i32()
        return [self._read_texture() for _ in range(count)]

    def _read_texture(self) -> dict:
        nid, flags, _ = self.node()
        if (nid, flags) != (15, 4):
            raise ValueError(f"expected Texture, got {nid}/{flags}")
        name = self.s()
        klass = self.s()
        bitmap = self.s()
        self.f32()                       # amount
        self.i32(); self.i32()           # type, map_type
        for _ in range(8):               # u/v offset+tiling, angle, blur, blur_offset, noise
            self.f32()
        self.f32()                       # noise_size
        self.i32()                       # noise_level
        self.f32()                       # noise_phase
        self.i32(); self.i32()           # invert, unknown
        self.i32(); self.i32()           # filter, channel
        subcount = self.i32()
        if subcount:
            self._read_texture_list(subcount)
        return {"name": name, "klass": klass, "bitmap": bitmap}

    # -- objects ----------------------------------------------------------
    def _read_object_list(self, scene: Scene):
        nid, flags, _ = self.node()
        if (nid, flags) != (18, 2):
            raise ValueError(f"expected ObjectList, got {nid}/{flags}")
        count = self.i32()
        for _ in range(count):
            oid, oflags, olen = self.node()
            start = self.p
            try:
                if oid == 2:
                    scene.objects.append(self._read_geometric(oflags))
                elif oid == 21:
                    scene.objects.append(self._read_attachment(oflags))
                elif oid == 5:
                    scene.objects.append(self._read_light(oflags))
                elif oid == 4:
                    scene.objects.append(self._read_camera(oflags))
                elif oid == 31:
                    scene.objects.append(self._read_rigidbody_collection(oflags))
                elif oid == 30:
                    scene.objects.append(self._read_simulation(oflags))
                elif oid == 42:
                    scene.objects.append(self._read_constraint_solver(oflags))
                elif oid == 49:
                    scene.objects.append(self._read_angular_dashpot(oflags))
                else:
                    if olen > 0:
                        self.p = start + olen
                    scene.objects.append(Element(kind=f"unknown({oid})"))
            except EOFError:
                raise
            except Exception as e:  # noqa: BLE001
                raise ValueError(f"object id={oid} at {start}: {e}") from e

    def _read_geometric(self, flags) -> Element:
        if flags != 4:
            raise ValueError(f"geometric flags {flags}")
        el = Element(kind="geometric")
        el.name = self.s()
        el.parent = self.s()
        el.extra["shade_verts"] = self.u8()
        el.tm = self._tm_node()
        el.mesh = self._read_mesh()
        return el

    def _read_mesh(self) -> Mesh:
        nid, flags, _ = self.node()
        if (nid, flags) != (16, 4):
            raise ValueError(f"expected Mesh, got {nid}/{flags}")
        m = Mesh()
        m.time = self.i32()
        pcount, fcount, tcount, ccount = self.i32(), self.i32(), self.i32(), self.i32()
        m.material_ref = self.i32()
        m.points = [self.vec3() for _ in range(pcount)]
        m.faces = [self.triplet() + (self.i32(),) for _ in range(fcount)]
        m.texcoords = [self.vec3() for _ in range(tcount)]
        if tcount:
            m.texfaces = [self.triplet() for _ in range(fcount)]
        if self.i32():                       # additional texture channels
            self._read_texture_channels()
        if ccount:
            for _ in range(ccount):
                self.vec3()                  # colors
            for _ in range(fcount):
                self.triplet()               # color faces
        m.normals = [self.vec3() for _ in range(fcount * 4)]
        m.backface_cull = self.i32()
        return m

    def _read_texture_channels(self):
        n = self.i32()
        for i in range(n):
            self.i32()                       # channel index
            tcount = self.i32()
            self.i32()
            fcount = self.i32()
            self.i32(); self.i32()
            self.i32(); self.i32()
            for _ in range(tcount):
                self.vec3()
            if tcount:
                for _ in range(fcount):
                    self.triplet()

    def _read_attachment(self, flags) -> Element:
        if flags != 2:
            raise ValueError(f"attachment flags {flags}")
        el = Element(kind="attachment")
        el.name = self.s()
        el.tm = self._tm_node()
        el.data = self.s()
        return el

    def _read_light(self, flags) -> Element:
        el = Element(kind="light")
        el.name = self.s()
        el.tm = self._tm_node()
        target = self.s()
        if target:
            self._tm_node()
        self.i32(); self.i32(); self.i32()
        self.color()
        self.f32(); self.f32()
        self.read(8)
        self.f32(); self.f32(); self.f32()
        self.u8(); self.read(3)
        return el

    def _read_camera(self, flags) -> Element:
        el = Element(kind="camera")
        el.name = self.s()
        el.tm = self._tm_node()
        if self.i32():
            self._tm_node()
        self.i32()
        for _ in range(6):
            self.f32()
        return el

    def _read_simulation(self, flags) -> Element:
        el = Element(kind="simulation")
        el.name = self.s()
        self.vec3()                 # gravity
        self.f32(); self.f32()      # worldscale, simtolerance
        self.i32()                  # resolver
        self.u8(); self.f32(); self.f32()
        self.u8(); self.f32(); self.f32()
        self.u8(); self.f32()
        self.i32()                  # collision pairs
        return el

    # -- physics ----------------------------------------------------------
    def _read_rigidbody_collection(self, flags) -> Element:
        if flags != 4:
            raise ValueError(f"rb collection flags {flags}")
        el = Element(kind="rigidbodies")
        el.name = self.s()
        paircount = self.i32()
        self.i32()                  # solver type
        el.extra["bodies"] = self._read_rigidbody_list()
        if paircount:
            self._read_disabled_pairs(paircount)
        return el

    def _read_rigidbody_list(self, sizehint: Optional[int] = None) -> list:
        nid, flags, _ = self.node()
        if (nid, flags) != (33, 2):
            raise ValueError(f"expected RigidBodyList, got {nid}/{flags}")
        count = self.i32()
        return [self._read_rigidbody() for _ in range(count)]

    def _read_rigidbody(self) -> dict:
        nid, flags, _ = self.node()
        if (nid, flags) != (32, 4):
            raise ValueError(f"expected RigidBody, got {nid}/{flags}")
        rb = {}
        rb["name"] = self.s()
        rb["mass"] = self.f32()
        rb["elasticity"] = self.f32()
        rb["friction"] = self.f32()
        rb["opt_lvl"] = self.f32()
        rb["unyielding"] = self.i32()
        rb["sim_geom"] = self.i32()
        rb["geom_proxy"] = self.s()
        rb["use_display_proxy"] = self.u8()
        rb["disable_collisions"] = self.u8()
        rb["inactive"] = self.u8()
        rb["display_proxy"] = self.s()
        rb["tm"] = self._tm_node()
        rb["geo_type"] = self.i32()
        childcount = self.i32()
        if childcount:
            rb["children"] = self._read_rigidbody_list(childcount)
        return rb

    def _read_disabled_pairs(self, sizehint):
        nid, flags, _ = self.node()
        if (nid, flags) != (51, 2):
            raise ValueError(f"expected DisabledCollisionPairs, got {nid}/{flags}")
        count = self.i32()
        for _ in range(count):
            self.s(); self.s()

    def _read_constraint_solver(self, flags) -> Element:
        el = Element(kind="constraints")
        el.name = self.s()
        self.f32()                  # threshold
        el.extra["rb_coll"] = self.s()
        n = self.i32()
        el.extra["items"] = []
        if n:
            self._read_constraint_list(el.extra["items"])
        return el

    def _read_constraint_list(self, out: list):
        nid, flags, _ = self.node()
        if (nid, flags) != (44, 2):
            raise ValueError(f"expected ConstraintList, got {nid}/{flags}")
        count = self.i32()
        for _ in range(count):
            cid, cflags, _ = self.node()
            if cid in (46, 47, 53):
                name = self.s()
                self._tm_node()
                b1 = self.s(); b2 = self.s()
                p1 = self.vec3()
                out.append({"type": {46: "pivot", 47: "hinge", 53: "p2p"}[cid],
                            "name": name, "b1": b1, "b2": b2, "p1": p1})
                if cid == 46:
                    self.vec3()
                    for _ in range(8):
                        self.f32()
                elif cid == 47:
                    self.vec3()
                    self.u8(); self.f32()
                    self.f32(); self.f32()
                # p2p: point2
                if cid == 53:
                    self.vec3()
            else:
                raise ValueError(f"unknown constraint {cid}")

    def _read_angular_dashpot(self, flags) -> Element:
        el = Element(kind="dashpot")
        el.name = self.s()
        self._tm_node()
        self.s(); self.s()
        self.f32(); self.f32()
        self.u8()
        for _ in range(4):
            self.f32()
        return el


# ---------------------------------------------------------------------------
def export_obj(scene: Scene, path: str, scale: float = 1.0):
    with open(path, "w") as f:
        f.write("# exported from Robot Arena 2 .gmf\n")
        vbase = 0
        for i, el in enumerate(scene.meshes):
            m = el.mesh
            name = el.name or f"mesh{i}"
            f.write(f"o {sanitize(name)}\n")
            for (x, y, z) in m.points:
                f.write(f"v {x*scale:.6f} {y*scale:.6f} {z*scale:.6f}\n")
            for (u, v, _w) in m.texcoords:
                f.write(f"vt {u:.6f} {1.0 - v:.6f}\n")   # flip V for OBJ
            for idx, face in enumerate(m.faces):
                a, b, c, _mat = face
                if m.texfaces and len(m.texfaces) == len(m.faces) and m.texcoords:
                    ta, tb, tc = m.texfaces[idx]
                    f.write(f"f {vbase+a+1}/{ta+1} {vbase+b+1}/{tb+1} {vbase+c+1}/{tc+1}\n")
                else:
                    f.write(f"f {vbase+a+1} {vbase+b+1} {vbase+c+1}\n")
            vbase += len(m.points)
        for el in scene.attachments:
            if el.tm:
                x, y, z = el.tm[3]
                f.write(f"# attachment {sanitize(el.name)} at {x:.4f} {y:.4f} {z:.4f} data={el.data!r}\n")


def sanitize(s: str) -> str:
    return "".join(c if c.isalnum() or c in "._-" else "_" for c in s) or "unnamed"


def cmd_info(paths):
    for path in paths:
        data = open(path, "rb").read()
        print(f"\n=== {path} ({len(data)} bytes) ===")
        try:
            s = GMIParser(data).parse()
        except Exception as e:  # noqa: BLE001
            print(f"  PARSE ERROR: {e}")
            continue
        print(f"  materials: {len(s.materials)}  objects: {len(s.objects)}")
        for m in s.materials:
            tex = ", ".join(t["bitmap"] for t in m.textures if t["bitmap"]) or "-"
            print(f"    mat {m.name!r} klass={m.klass!r} tex=[{tex}]")
        for o in s.objects:
            if o.mesh:
                mm = o.mesh
                print(f"    mesh {o.name!r} verts={len(mm.points)} faces={len(mm.faces)} "
                      f"uv={len(mm.texcoords)} mat={mm.material_ref}")
            elif o.kind == "attachment":
                print(f"    attach {o.name!r} data={o.data!r}")
            elif o.kind == "rigidbodies":
                bodies = o.extra.get("bodies", [])
                print(f"    rigidbodies {o.name!r}: {len(bodies)} bodies")
                for rb in bodies:
                    print(f"        rb {rb['name']!r} mass={rb['mass']:.3f} geo_type={rb['geo_type']}")
            elif o.kind == "constraints":
                print(f"    constraints {o.name!r}: {len(o.extra.get('items', []))} items "
                      f"-> {[i['type'] for i in o.extra.get('items', [])]}")
            elif o.kind != "unknown(4)":
                print(f"    {o.kind} {o.name!r}")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p_info = sub.add_parser("info")
    p_info.add_argument("files", nargs="+")
    p_obj = sub.add_parser("obj")
    p_obj.add_argument("file")
    p_obj.add_argument("out")
    p_obj.add_argument("--scale", type=float, default=1.0)
    a = ap.parse_args(argv)

    if a.cmd == "info":
        cmd_info(a.files)
    elif a.cmd == "obj":
        s = GMIParser(open(a.file, "rb").read()).parse()
        export_obj(s, a.out, a.scale)
        print(f"wrote {a.out}: {len(s.meshes)} meshes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
