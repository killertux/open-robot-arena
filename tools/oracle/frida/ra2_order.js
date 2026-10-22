// Frida agent: what orders the contacts in the ORIGINAL (docs/29 section 12u).
// Function-entry/exit hooks only.
//   C  [C,coll,type,u50,body,mopp,u1c4,tree,geom,nv,ntri,verts,tris,leaves]
//      every collidable in the broadphase's lists (0x559d30's object: +0x24/+0x28
//      the active collidables, +0x34/+0x38 the second list, +0x208/+0x20c the
//      fixed list's projections), once: its shape type (+0x18), +0x50, body
//      (+0x1c), MOPP (+0x1c0), +0x1c4, OBB tree (+0x1c8, whose first word is the root), geometry (+0x1bc:
//      +0x18 vertex count, +0x1c vertices 16 bytes apart, +0x24 triangle count,
//      +0x28 triangles 3 x u16), and the tree's leaves' triangles depth first,
//      child +0 before child +4 (leaf triangle pointer at +0x60), then the
//      collidable's first 0x1c0 bytes as floats.
//   L  [L,seq,[[min,max,coll],...] x3]  the broadphase's three projected lists
//      at 0x559d30's exit (+0x1e8, +0x1f8, +0x208), every LIST_EVERY-th call
//   X  [X,seq,[[coll,box(8),xf(16),bodyc(4),bodyxf(12)],...]] every 50th call
//   Q  [Q,seq,coll,[tri...]]  0x566a80 MOPP query: the triangles it returns, in order
//      (thiscall, 5th stack argument the {ptr,count} list, 8-byte entries)
//   R  [R,seq,[[collA,collB,point(3),dist],...]]  0x53ed80 resolver entry (all contacts)
//   I  [I,seq,[[collA,collB,point(3)],...]]  0x559720 entry: one island's list
//   P  [P,seq,a,b]  0x55a750 entry: a pair offered to the broadphase's list (args)
'use strict';
let seq = 0, rows = [];
const LIST_EVERY = (typeof LIST_EVERY_N === 'number') ? LIST_EVERY_N : 1;
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const known = new Set();
const floats = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
function learn(c) {
  const k = c.toString();
  if (known.has(k) || c.isNull()) return;
  known.add(k);
  try {
    const g = c.add(0x1bc).readPointer();
    let nv = 0, nt = 0, verts = [], tris = [], leaves = [];
    let tb = ptr(0);
    if (!g.isNull()) {
      nv = g.add(0x18).readS32(); nt = g.add(0x24).readS32();
      const vb = g.add(0x1c).readPointer(); tb = g.add(0x28).readPointer();
      if (nv > 0 && nv < 20000 && !vb.isNull()) for (let i = 0; i < nv; i++) verts.push(vec(vb.add(16 * i)));
      if (nt > 0 && nt < 20000 && !tb.isNull()) for (let i = 0; i < nt; i++) {
        const t = tb.add(6 * i); tris.push([t.readU16(), t.add(2).readU16(), t.add(4).readU16()]);
      }
    }
    const tree = c.add(0x1c8).readPointer();
    if (!tree.isNull() && !tree.readPointer().isNull()) {
      const st = [tree.readPointer()];
      let guard = 0;
      while (st.length && guard++ < 40000) {
        const n = st.pop();
        const l = n.readPointer(), r = n.add(4).readPointer();
        if (l.isNull() && r.isNull()) {
          const t = n.add(0x60).readPointer();
          leaves.push(tb.isNull() ? t.toString() : t.sub(tb).toInt32() / 6);
        } else {
          if (!r.isNull()) st.push(r);
          if (!l.isNull()) st.push(l);
        }
      }
    }
    push(['C', k, c.add(0x18).readS32(), c.add(0x50).readU32(), c.add(0x1c).readPointer().toString(),
          c.add(0x1c0).readPointer().toString(), c.add(0x1c4).readPointer().toString(), tree.toString(),
          g.toString(), nv, nt, verts, tris, leaves, floats(c, 0x1c0)]);
  } catch (e) { push(['E', k, String(e)]); }
}
function vecOf(b, e, stride, get) {
  const o = [];
  if (b.isNull()) return o;
  for (let p = b; p.compare(e) < 0; p = p.add(stride)) o.push(get(p));
  return o;
}
let calls = 0;
Interceptor.attach(ptr('0x559d30'), {
  onEnter() { this.bp = this.context.ecx; },
  onLeave() {
    const bp = this.bp;
    try {
      for (const off of [0x24, 0x34]) for (const c of vecOf(bp.add(off).readPointer(), bp.add(off + 4).readPointer(), 4, p => p.readPointer())) learn(c);
      const lists = [];
      for (const off of [0x1e8, 0x1f8, 0x208]) {
        const l = vecOf(bp.add(off).readPointer(), bp.add(off + 4).readPointer(), 12, p => [f(p), f(p.add(4)), p.add(8).readPointer().toString()]);
        for (const e of l) learn(ptr(e[2]));
        lists.push(l);
      }
      if (calls++ % LIST_EVERY === 0) {
        push(['L', ++seq, lists]);
        // X: every listed collidable's box (+0x20..+0x40) and transform
        // (+0x138, 16 floats), with its body's centre (+0x30) and rotation (+0xcc)
        if ((calls % 50) === 1) {
          const xs = [];
          for (const l of lists) for (const e of l) {
            const c = ptr(e[2]), b = c.add(0x1c).readPointer();
            xs.push([e[2], floats(c.add(0x20), 0x20), floats(c.add(0x138), 0x40), b.isNull() ? null : floats(b.add(0x30), 0x10), b.isNull() ? null : floats(b.add(0xcc), 0x30)]);
          }
          push(['X', ++seq, xs]);
        }
      }
    } catch (e) { push(['E', ++seq, 'bp ' + String(e)]); }
  }
});
Interceptor.attach(ptr('0x55a750'), { onEnter() {
  const sp = this.context.esp;
  push(['P', ++seq, sp.add(4).readPointer().toString(), sp.add(8).readPointer().toString()]);
} });
Interceptor.attach(ptr('0x566a80'), {
  onEnter() { const sp = this.context.esp; this.out = sp.add(0x14).readPointer(); this.coll = this.context.ecx; },
  onLeave() {
    try {
      const b = this.out.readPointer(), n = this.out.add(4).readS32();
      const o = [];
      for (let i = 0; i < n && i < 512; i++) o.push(b.add(8 * i).readS32());
      push(['Q', ++seq, o]);
    } catch (e) { push(['E', ++seq, 'Q ' + String(e)]); }
  }
});
function contacts(list) {
  const b = list.add(4).readPointer(), e = list.add(8).readPointer();
  return vecOf(b, e, 0x74, r => [r.add(8).readPointer().toString(), r.add(0xc).readPointer().toString(), vec(r.add(0x20)), f(r.add(0x54))]);
}
Interceptor.attach(ptr('0x53ed80'), { onEnter() {
  try { push(['R', ++seq, contacts(this.context.esp.add(4).readPointer())]); } catch (e) { push(['E', ++seq, 'R ' + String(e)]); }
} });
Interceptor.attach(ptr('0x559720'), { onEnter() {
  try { push(['I', ++seq, contacts(this.context.esp.add(4).readPointer())]); } catch (e) { push(['E', ++seq, 'I ' + String(e)]); }
} });
setInterval(flush, 200);
rpc.exports = { flush() { flush(); } };
