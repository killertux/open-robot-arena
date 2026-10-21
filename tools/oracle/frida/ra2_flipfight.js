// Frida agent: ra2_fightcontacts.js plus every hinge motor setting (one seq),
// to follow a flipper's strokes against the bodies it strikes (docs/29
// section 12p).  Rows as ra2_fightcontacts.js, plus
//   M  [M,seq,motor,velocity,max torque]  0x4815c0 hinge motor set, on change
//      (as ra2_weapons.js; a burst motor's fire and return are its two settings)
// Frida agent: the contact points Havok 1.7 hands its resolver in the ORIGINAL,
// with the bodies they push, every step piece.  Function-entry hooks only (the
// physics code is x87-heavy; mid-function hooks corrupt its stack).
//   R  [R,seq,tol,contacts]  0x53ed80 resolver driver entry, thiscall(resolver, list, tol)
//      contacts = [[bodyA,bodyB,normal(3),point(3),distance,muA,muB,eA,eB,collA,collB],...]
//      (0x74-byte records: +8/+0xc collidables, collidable+0x1c body, +0x10 normal,
//       +0x20 point, +0x54 distance, +0x58/+0x5c mu, +0x60/+0x64 e)
//   B  [B,seq,body,mass,com(3),vel(3),angvel(3),rot(9)]  every movable body seen in any
//      contact so far and every MagicAction's chassis, once per R
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
const lastMotor = {};
Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  try {
    const m = this.context.ecx.toString(), sp = this.context.esp, v = f(sp.add(4)), t = f(sp.add(8));
    if (lastMotor[m] && lastMotor[m][0] === v && lastMotor[m][1] === t) return;
    lastMotor[m] = [v, t];
    push(['M', ++seq, m, v, t]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
