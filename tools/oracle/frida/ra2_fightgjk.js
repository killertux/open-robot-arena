// Frida agent: ra2_contacts.js and the priced hits of ra2_fight_hits.js in
// one script (one seq), for replaying the contact stream through the event
// filter (docs/29 section 12m).  Rows: R, B, A, G, P as ra2_contacts.js; Q (the
// computeDamage row ra2_fight_hits.js calls P) and H as ra2_fight_hits.js.
// Frida agent: the contact points Havok 1.7 hands its resolver in the ORIGINAL,
// with the bodies they push, every step piece.  Function-entry hooks only (the
// physics code is x87-heavy; mid-function hooks corrupt its stack).
//   R  [R,seq,tol,contacts]  0x53ed80 resolver driver entry, thiscall(resolver, list, tol)
//      contacts = [[bodyA,bodyB,normal(3),point(3),distance,muA,muB,eA,eB,collA,collB],...]
//      (0x74-byte records: +8/+0xc collidables, collidable+0x1c body, +0x10 normal,
//       +0x20 point, +0x54 distance, +0x58/+0x5c mu, +0x60/+0x64 e)
//   B  [B,seq,body,mass,com(3),vel(3),angvel(3),rot(9)]  every movable body seen in any
//      contact so far and every MagicAction's chassis, once per R
//   V  [V,seq,body,vel(3),angvel(3)]  every known body as the resolver leaves it (0x53ed80 exit)
//   A  [A,seq,h,rollbacks]   0x52a990 actions entry: one per accepted step piece
//                            (rollbacks = "Backstep/Roll Back" counter DAT_00719724)
//   G  [G,seq,chassis,[[thrust,touching],...]]  0x4856f0 MagicAction apply entry
//   P  [P,seq,body,[floats 0..0x200]]  once per body, when it is first seen: its raw
//      floats (+0x30 centre of mass, +0xcc transform, +0x10c world inverse inertia,
//      +0x190 mass, +0x19c local inertia; docs/29 sections 12e/12h)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const MAX = (typeof MAX_CONTACTS === 'number') ? MAX_CONTACTS : 400;
const known = new Map();
const floats = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
function learn(p) {
  const k = p.toString();
  if (known.has(k)) return;
  known.set(k, p);
  push(['P', seq, k, floats(p, 0x200)]);
}

Interceptor.attach(ptr('0x53ed80'), { onEnter() {
  try {
    const sp = this.context.esp;
    const list = sp.add(4).readPointer();
    const tol = f(sp.add(8));
    const b = list.add(4).readPointer(), e = list.add(8).readPointer();
    if (b.isNull()) { push(['R', ++seq, tol, []]); return; }
    const out = [];
    let n = 0;
    for (let r = b; r.compare(e) < 0 && n < MAX; r = r.add(0x74), n++) {
      const ca = r.add(8).readPointer(), cb = r.add(0xc).readPointer();
      const ba = ca.add(0x1c).readPointer(), bb = cb.add(0x1c).readPointer();
      for (const p of [ba, bb]) if (!known.has(p.toString()) && f(p.add(0x190)) > 0) learn(p);
      out.push([ba.toString(), bb.toString(), vec(r.add(0x10)), vec(r.add(0x20)), f(r.add(0x54)),
                f(r.add(0x58)), f(r.add(0x5c)), f(r.add(0x60)), f(r.add(0x64)), ca.toString(), cb.toString()]);
    }
    push(['R', ++seq, tol, out]);
    for (const [k, p] of known) {
      const xf = p.add(0xcc), rot = [];
      for (let i = 0; i < 9; i++) rot.push(f(xf.add(4 * i)));
      push(['B', seq, k, f(p.add(0x190)), vec(p.add(0x30)), vec(p.add(0x54)), vec(p.add(0x64)), rot]);
    }
  } catch (err) { push(['E', ++seq, String(err)]); }
},
// V  [V,seq,body,vel(3),angvel(3)]  every known body as the resolver leaves it
//    (docs/29 section 12aj: the post-resolver velocities for a step-by-step replay)
onLeave() {
  try {
    for (const [k, p] of known) push(['V', seq, k, vec(p.add(0x54)), vec(p.add(0x64))]);
  } catch (err) { push(['E', seq, String(err)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', ++seq, f(this.context.esp.add(4)), ptr('0x719724').readU32()]); } });
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const t = this.context.ecx;
    const wrap = t.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    const b = t.add(0x24).readPointer(), e = t.add(0x28).readPointer();
    const ws = [];
    for (let r = b; r.compare(e) < 0; r = r.add(0x50)) ws.push([r.add(4).readS32(), r.add(8).readU8()]);
    learn(body);
    push(['G', ++seq, body.toString(), ws]);
  } catch (err) { push(['E', ++seq, String(err)]); }
} });
Interceptor.attach(ptr('0x489470'), { onEnter() {
  try {
    const sp = this.context.esp, rec = sp.add(8).readPointer();
    const r = []; for (let i = 0; i < 6; i++) r.push(f(rec.add(4 * i)));
    push(['Q', ++seq, this.context.ecx.toString(), r, sp.add(4).readU32()]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x417330'), { onEnter() {
  try {
    const sp = this.context.esp;
    let loc = null;
    try { const p = sp.add(24).readPointer(); loc = [f(p), f(p.add(4)), f(p.add(8))]; } catch (e) {}
    push(['H', ++seq, sp.add(4).readS32(), sp.add(8).readPointer().toString(), sp.add(12).readS32(),
          sp.add(16).readPointer().toString(), f(sp.add(20)), loc]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
// J  [J,seq,coll,count,adj,startBefore,startAfter,n,ia,ib,dist,normal,onA,onB,tri(9),xf(16),shape]
//    0x55c390, the convex x triangle distance query (docs/29 section 12ak): the hull's collidable, vertex
//    count, whether the shape has an adjacency list (+0x1d0), the hill-climb's start index before and
//    after, the final simplex (hull / triangle vertex indices), the result block and the triangle.
//    S  [S,seq,shape,count,[vertices],[adjacency lists]]  once per hull shape seen by a J row.
// F  [F,seq,collA,collB,rec,passed,type]  0x52d4f0, the world's event filter: one row per call
//    (docs/29 section 12ak: who calls it, per point or per pair).
const JMAX = (typeof MAX_J === 'number') ? MAX_J : 200000;
let nJ = 0;
const shapes = new Set();
Interceptor.attach(ptr('0x55c390'), {
  onEnter() {
    this.ok = false;
    try {
      if (nJ >= JMAX) return;
      const sp = this.context.esp;
      const aobj = sp.add(4).readPointer(), bobj = sp.add(8).readPointer();
      this.out = sp.add(12).readPointer(); this.st = sp.add(16).readPointer();
      const c = aobj.readPointer();
      const shape = c.add(0x1bc).readPointer();
      const count = shape.add(0x18).readU32();
      if (count < 8 || count > 80) return;
      this.aobj = aobj; this.c = c; this.shape = shape; this.count = count;
      this.adj = !c.add(0x1d0).readPointer().isNull();
      this.start0 = aobj.add(4).readU16();
      this.tv = bobj.readPointer();
      this.ok = true;
    } catch (e) { push(['E', seq, 'J in ' + e]); }
  },
  onLeave() {
    if (!this.ok) return;
    try {
      const out = this.out, st = this.st, n = st.add(0x20).readU32();
      const vbase = this.shape.add(0x1c).readPointer();
      const ia = [], ib = [];
      for (let i = 0; i < n && i < 4; i++) {
        ia.push(st.add(4 * i).readPointer().sub(vbase).toInt32() / 16);
        ib.push(st.add(0x10 + 4 * i).readPointer().sub(this.tv).toInt32() / 16);
      }
      const tri = []; for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) tri.push(f(this.tv.add(16 * i + 4 * j)));
      const xf = floats(this.c.add(0x138), 64);
      const key = this.shape.toString();
      if (!shapes.has(key)) {
        shapes.add(key);
        const vs = []; for (let i = 0; i < this.count; i++) for (let j = 0; j < 3; j++) vs.push(f(vbase.add(16 * i + 4 * j)));
        let adj = null;
        try {
          const tbl = this.c.add(0x1d0).readPointer().add(4).readPointer();
          adj = [];
          for (let i = 0; i < this.count; i++) {
            const e = tbl.add(16 * i), b = e.add(4).readPointer(), en = e.add(8).readPointer(), l = [];
            for (let q = b; q.compare(en) < 0 && l.length < 64; q = q.add(2)) l.push(q.readU16());
            adj.push(l);
          }
        } catch (e) { adj = String(e); }
        push(['S', seq, key, this.count, vs, adj]);
      }
      const dist = f(out.add(0x44));
      if (!(dist < 0.25 && dist > 0) || n > 3) return;
      nJ++;
      push(['J', seq, this.c.toString(), this.count, this.adj ? 1 : 0, this.start0, this.aobj.add(4).readU16(), n, ia, ib,
            dist, vec(out), vec(out.add(0x20)), vec(out.add(0x30)), tri, xf, key]);
    } catch (e) { push(['E', seq, 'J out ' + e]); }
  } });
Interceptor.attach(ptr('0x52d4f0'), {
  onEnter() {
    try {
      const ev = this.context.esp.add(4).readPointer();
      this.row = ['F', seq, ev.add(0x10).readPointer().toString(), ev.add(0x14).readPointer().toString(),
                  ev.add(0xc).readPointer().toString(), 0, ev.add(8).readU32()];
    } catch (e) { this.row = null; push(['E', seq, 'F in ' + e]); }
  },
  onLeave(ret) { if (this.row) { this.row[5] = ret.toInt32() & 0xff; push(this.row); } } });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
