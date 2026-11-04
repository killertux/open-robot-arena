// Frida agent: every collidable of the robots in the ORIGINAL, with its shape (docs/29 section 12at).
// Derived from ra2_order.js' C rows (0x559d30's lists).  Function-entry/exit hooks only.
//   C  [C,coll,type +0x18,tag +0x50,body +0x1c,nv,ntri,verts,tris,box(+0x20..+0x40, 8 floats),
//       xf(+0x138, 16 floats),bodyMass(+0x190),bodyCom(+0x30,4),bodyRot(+0xcc,12),raw(+0..+0x1c0 floats),
//       shapeHeader(geometry +0..+0x40 as u32)]   once per collidable (movable or fixed)
//   Every body of every component (K rows' wrapper +8) lists its collidables at +0x250 (begin) / +0x254
//   (end); the broadphase's lists hold a body's pieces only once they are active, so the dump walks the
//   bodies instead: pass 1 at 1 s of the match clock, pass 2 at 12 s, pass 3 from rpc flush() (the end).
//   The contact pairs (RP) are counted only while the match is on.
//   K  [K,seq,player,id,comp,wrapperBodies,raw(comp +0..+0x100 as u32)]   0x4dd760 leave: id -> component,
//       with the bodies of the component's wrappers (+0x24 vector; wrapper +8 / +0x10)
//   MK [MK,seq,player]   rpc mark(player)
//   RP [RP,seq,tagA,tagB,n,minDist,nBelow0,nAbove,distSum]  rpc flush(): the contact pairs of the resolver (0x53ed80)
//       by the two collidables' tags (+0x50), over the whole capture
//   F  [F,seq,tagA,tagB,passed,distance,point,time]  0x52d4f0, the world's event filter, one row per call between
//      two robot collidables in the first 30 s of the match (the event's two collidables by tag, the contact record's distance and point)
//   H  [H,seq,attPlayer,attComp,vicPlayer,vicComp,amount,point,normal]  0x417330: a priced hit (the components'
//      pointers; the K rows name them), in step with the F rows of the same seq
//   R  [R,seq,object,partner,interval,threshold,type]  0x52d850 entry (12l): one entry appended to the object's
//      list in the event filter: the objects are collidables (compare the C rows' pointers), registered while the
//      match loads
//   A  [A,seq,h]         0x52a990 entry: one per accepted step piece
//   P  [P,seq,time,leaving,[coll,+0x50,+0x54,+0x58],[coll,..]]  0x4a4780: every phantom-pair event (docs/29 12au)
//   W  [W,seq,time,[direction,id,robot,chassis]]  0x4c9050: the arena's ZoneEvent handed to Python
'use strict';
let seq = 0, rows = [], player = -1;
function push(r) { rows.push(r); if (rows.length >= 64) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const floats = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
const words = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readU32()); return o; };
const known = new Set();
const bodiesSeen = new Set();
let pass = 0, steps = 0;
function learn(c) {
  const k = c.toString();
  if (known.has(pass + k) || c.isNull()) return;
  known.add(pass + k);
  try {
    const g = c.add(0x1bc).readPointer();
    let nv = 0, nt = 0, verts = [], tris = [], hdr = null;
    if (!g.isNull()) {
      hdr = words(g, 0x40);
      nv = g.add(0x18).readS32(); nt = g.add(0x24).readS32();
      const vb = g.add(0x1c).readPointer(), tb = g.add(0x28).readPointer();
      if (nv > 0 && nv < 20000 && !vb.isNull()) for (let i = 0; i < nv; i++) verts.push(vec(vb.add(16 * i)));
      if (nt > 0 && nt < 20000 && !tb.isNull()) for (let i = 0; i < nt; i++) {
        const t = tb.add(6 * i); tris.push([t.readU16(), t.add(2).readU16(), t.add(4).readU16()]);
      }
    }
    const b = c.add(0x1c).readPointer();
    push(['C', k, c.add(0x18).readS32(), c.add(0x50).readU32(), b.toString(), nv, nt, verts, tris,
          floats(c.add(0x20), 0x20), floats(c.add(0x138), 0x40), b.isNull() ? 0 : f(b.add(0x190)),
          b.isNull() ? null : floats(b.add(0x30), 0x10), b.isNull() ? null : floats(b.add(0xcc), 0x30),
          floats(c, 0x1c0), hdr, pass]);
  } catch (e) { push(['E', ++seq, 'C ' + String(e)]); }
}
const list = (b, e) => { const o = []; if (b.isNull()) return o; for (let p = b; p.compare(e) < 0 && o.length < 4096; p = p.add(4)) o.push(p.readPointer()); return o; };
Interceptor.attach(ptr('0x559d30'), {
  onEnter() { this.bp = this.context.ecx; },
  onLeave() {
    const bp = this.bp;
    try {
      for (const off of [0x24, 0x34]) for (const c of list(bp.add(off).readPointer(), bp.add(off + 4).readPointer())) learn(c);
      for (const off of [0x1e8, 0x1f8, 0x208]) {
        const b = bp.add(off).readPointer(), e = bp.add(off + 4).readPointer();
        if (!b.isNull()) for (let p = b; p.compare(e) < 0; p = p.add(12)) learn(p.add(8).readPointer());
      }
    } catch (e) { push(['E', ++seq, 'bp ' + String(e)]); }
  }
});
const pairs = new Map();
Interceptor.attach(ptr('0x53ed80'), { onEnter() {
  try {
    const list = this.context.esp.add(4).readPointer();
    const b = list.add(4).readPointer(), e = list.add(8).readPointer();
    if (b.isNull()) return;
    let n = 0;
    if (!fighting) return;
    for (let r = b; r.compare(e) < 0 && n < 400; r = r.add(0x74), n++) {
      const ca = r.add(8).readPointer(), cb = r.add(0xc).readPointer();
      const ta = ca.add(0x50).readU32(), tb = cb.add(0x50).readU32();
      const k = ta + ',' + tb, d = f(r.add(0x54));
      let q = pairs.get(k);
      if (!q) { q = { n: 0, min: 1e9, neg: 0, sum: 0 }; pairs.set(k, q); }
      q.n++; q.sum += d; if (d < q.min) q.min = d; if (d < 0) q.neg++;
    }
  } catch (e) {}
} });
Interceptor.attach(ptr('0x52d4f0'), {
  onEnter() {
    this.row = null;
    try {
      if (!fighting) return;
      const ev = this.context.esp.add(4).readPointer();
      const ca = ev.add(0x10).readPointer(), cb = ev.add(0x14).readPointer(), rec = ev.add(0xc).readPointer();
      const ta = ca.add(0x50).readU32(), tb = cb.add(0x50).readU32();
      if (ta < 7 || tb < 7 || clock() > 30) return;   // robot pieces only (the arena's tags are 0..6), the first 30 s
      this.row = ['F', ++seq, ta, tb, 0, f(rec.add(0x54)), vec(rec.add(0x20)).map(x => Math.round(x * 1e4) / 1e4), clock()];
    } catch (e) { this.row = null; }
  },
  onLeave(ret) { if (this.row) { this.row[4] = ret.toInt32() & 0xff; push(this.row); } } });
Interceptor.attach(ptr('0x417330'), { onEnter() {
  try {
    const sp = this.context.esp;
    let loc = null, nrm = null;
    try { loc = vec(sp.add(24).readPointer()); } catch (e) {}
    try { nrm = vec(sp.add(28).readPointer()); } catch (e) {}
    push(['H', ++seq, sp.add(4).readS32(), sp.add(8).readPointer().toString(), sp.add(12).readS32(),
          sp.add(16).readPointer().toString(), f(sp.add(20)), loc, nrm]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x52d850'), { onEnter() {
  try {
    const sp = this.context.esp;
    push(['R', ++seq, sp.add(4).readPointer().toString(), sp.add(8).readPointer().toString(), f(sp.add(12)), f(sp.add(16)), sp.add(20).readU32()]);
  } catch (e) { push(['E', ++seq, 'R ' + String(e)]); }
} });
const seenK = new Set();
Interceptor.attach(ptr('0x4dd760'), {
  onEnter() { this.id = this.context.esp.add(4).readS32(); this.robot = this.context.ecx.toString(); },
  onLeave(ret) {
    try {
      if (ret.isNull()) return;
      const key = this.robot + '/' + this.id + '/' + ret + '/' + player;
      if (seenK.has(key)) return;
      seenK.add(key);
      const wb = [];
      try {
        for (const w of list(ret.add(0x24).readPointer(), ret.add(0x28).readPointer())) {
          const b = w.add(8).readPointer();
          wb.push([w.toString(), b.toString(), w.add(0x10).readPointer().toString()]);
          if (b.compare(ptr(0x10000)) > 0) bodiesSeen.add(b.toString());
        }
      } catch (e) {}
      push(['K', ++seq, player, this.id, ret.toString(), wb, words(ret, 0x100)]);
    } catch (e) { push(['E', ++seq, 'K ' + String(e)]); }
  }
});
function dumpBodies() {
  pass++;
  for (const bs of bodiesSeen) {
    try {
      const b = ptr(bs);
      for (const c of list(b.add(0x250).readPointer(), b.add(0x254).readPointer())) learn(c);
    } catch (e) { push(['E', ++seq, 'bodies ' + String(e)]); }
  }
  push(['PASS', ++seq, pass]);
}
// the match clock (0x7162c8: +0x8e fighting, +0xe4 elapsed)
const clock = () => { const m = ptr('0x7162c8'); return m.add(0x8e).readU8() ? f(m.add(0xe4)) : -1; };
let fighting = false, passes = [1.0, 12.0, 30.0];
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', ++seq, f(this.context.esp.add(4))]);
  try {
    const t = clock();
    fighting = t >= 0;
    if (t >= 0 && passes.length && t >= passes[0]) { passes.shift(); dumpBodies(); }
  } catch (e) {}
} });
const PAIR = ptr('0x6abf6c').readU32();
Interceptor.attach(ptr('0x4a4780'), { onEnter() {
  try {
    if (!fighting) return;
    const ev = this.context.esp.add(4).readPointer();
    if (ev.add(8).readU32() !== PAIR) return;
    const side = c => c.isNull() ? null : [c.toString(), c.add(0x50).readU32(), c.add(0x54).readU32(), c.add(0x58).readU8()];
    push(['P', ++seq, clock(), ev.add(0x14).readU32(), side(ev.add(0xc).readPointer()), side(ev.add(0x10).readPointer())]);
  } catch (e) { push(['E', ++seq, 'P ' + String(e)]); }
} });
Interceptor.attach(ptr('0x4c9050'), { onEnter() {
  try {
    const sp = this.context.esp, a = [];
    for (let i = 1; i <= 4; i++) a.push(sp.add(4 * i).readS32());
    push(['W', ++seq, clock(), a]);
  } catch (e) { push(['E', ++seq, 'W ' + String(e)]); }
} });
setInterval(flush, 200);
function dumpPairs() {
  for (const [k, q] of pairs) { const [a, b] = k.split(',').map(Number); push(['RP', ++seq, a, b, q.n, q.min, q.neg, q.sum]); }
}
rpc.exports = { flush() { dumpBodies(); dumpPairs(); flush(); }, mark(p) { player = p; push(['MK', ++seq, p]); } };
