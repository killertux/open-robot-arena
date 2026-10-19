// Frida agent: how the ORIGINAL assembled each robot (docs/29 section 8).
//
// The per-robot power step 0x4dc3f0 (thiscall, ecx = Robot) collects the robots;
// Robot +0x18 is the component list (begin +0x1c, end +0x20, 4-byte handles,
// *handle = component).  A component's +0x1c is its gi3DObject node.  gi3DObject
// (vtable 0x6a5598, mesh instances 0x6af4b8): +0xc local 4x4 and +0x4c world
// 4x4 (row-major, row 3 = translation), +0xbc/+0xc0 child vector, +0xe0 parent,
// +0xd0/+0xd4 name (char*, length).  The scene under the robot's root has one
// node per Havok rigid body ("Chassis Display" for the chassis body, the driven
// body's top rigid body name otherwise) whose world is the body's pose; the
// component meshes hang below them as "<object> Instance" nodes.
//
// dump() returns, per robot: the component list (address, RTTI class, node) and
// the whole node tree with local and world matrices, plus every Havok rigid body
// the integrator touched (0x520fb0 pack, ecx = body: +0x30 position, +0xcc 3x4
// rotation|position, +0x190 mass).
'use strict';
const robots = new Map();
// The first power step of each robot: its scene as built, before the joints
// and contacts have moved anything (one Havok step of free fall at most).
const first = new Map();
Interceptor.attach(ptr('0x4dc3f0'), { onEnter() {
  const r = this.context.ecx, k = r.toString();
  if (!robots.has(k)) { robots.set(k, r); try { first.set(k, snapshot(r)); } catch (e) { first.set(k, { err: String(e) }); } }
} });
const bodies = new Map();
Interceptor.attach(ptr('0x520fb0'), { onEnter() { bodies.set(this.context.ecx.toString(), this.context.ecx); } });
function rtti(vt) {
  try { return vt.sub(4).readPointer().add(12).readPointer().add(8).readCString(); } catch (e) { return ''; }
}
const M = (p) => { const a = []; for (let i = 0; i < 16; i++) a.push(p.add(4 * i).readFloat()); return a; };
function node(p, depth) {
  const r = { p: p.toString(), vt: p.readPointer().toString(), kind: p.add(4).readU32(),
              local: M(p.add(0xc)), world: M(p.add(0x4c)), kids: [] };
  try { const n = p.add(0xd4).readU32(); if (n > 0 && n < 200) r.name = p.add(0xd0).readPointer().readCString(n); } catch (e) {}
  if (depth > 16) return r;
  const b = p.add(0xbc).readPointer(), e = p.add(0xc0).readPointer();
  for (let q = b; q.compare(e) < 0 && r.kids.length < 128; q = q.add(4)) r.kids.push(node(q.readPointer(), depth + 1));
  return r;
}
function snapshot(r) {
      const comps = [];
      const b = r.add(0x1c).readPointer(), e = r.add(0x20).readPointer();
      for (let p = b; p.compare(e) < 0 && comps.length < 128; p = p.add(4)) {
        try {
          const c = p.readPointer().readPointer();
          comps.push({ c: c.toString(), cls: rtti(c.readPointer()), node: c.add(0x1c).readPointer().toString() });
        } catch (err) { comps.push({ err: String(err) }); }
      }
      let tree = null;
      try { tree = node(ptr(comps[0].node).add(0xe0).readPointer(), 0); } catch (err) { tree = { err: String(err) }; }
      return { comps: comps, tree: tree, t: Date.now() };
}
rpc.exports = {
  dump() {
    const out = { robots: [], bodies: [] };
    robots.forEach((r, k) => {
      const s = snapshot(r);
      out.robots.push({ robot: k, comps: s.comps, tree: s.tree, first: first.get(k) || null });
    });
    bodies.forEach((b, k) => {
      try {
        const xf = []; for (let i = 0; i < 12; i++) xf.push(b.add(0xcc + 4 * i).readFloat());
        out.bodies.push({ b: k, mass: b.add(0x190).readFloat(), pos: [0, 4, 8].map(o => b.add(0x30 + o).readFloat()), xf: xf });
      } catch (err) {}
    });
    return out;
  } };
