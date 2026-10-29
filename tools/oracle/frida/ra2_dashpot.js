// Frida agent: FlexTop's table in the ORIGINAL (docs/29 section 12j).  The table
// (`floor_collision`, 400 kg) hangs from the fixed `anchor03` by a point-to-point
// constraint and two angular dashpots; this records what the dashpots are given
// and how the table moves.  Function-entry hooks only.
//   A  [A,seq,h,rollbacks]  0x52a990 actions entry: one per accepted step piece
//   D  [D,seq,this,bodyA,bodyB,arg1,t1i,t1f,t2i,t2f,[floats this+0..+0x50]]
//      0x52cc00 angular dashpot apply, thiscall(dashpot, world?, int, float); the
//      first DFULL calls in full, then every call as a short D row
//      [D,seq,this,t1i,t1f,t2i,t2f]
//   T  [T,seq,body,com(3),vel(3),angvel(3),rot(9),quat(4)]  the dashpot's body B
//      (and every chassis MagicAction names) after each D on the first dashpot
//   P  [P,seq,body,[floats 0..0x200]]  once per body (+0x30 centre, +0x40 quaternion,
//      +0xcc transform, +0x10c world inverse inertia, +0x190 mass, +0x19c local inertia)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const floats = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
const DFULL = (typeof DASHPOT_FULL === 'number') ? DASHPOT_FULL : 600;
const known = new Map();
const chassis = new Map();
let first = null, nfull = 0;
function learn(p) {
  const k = p.toString();
  if (known.has(k) || p.isNull()) return;
  known.set(k, p);
  push(['P', seq, k, floats(p, 0x200)]);
}
function state(p) {
  const xf = p.add(0xcc), rot = [];
  for (let i = 0; i < 9; i++) rot.push(f(xf.add(4 * i)));
  return ['T', seq, p.toString(), vec(p.add(0x30)), vec(p.add(0x54)), vec(p.add(0x64)), rot,
          [f(p.add(0x40)), f(p.add(0x44)), f(p.add(0x48)), f(p.add(0x4c))]];
}
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', ++seq, f(this.context.esp.add(4)), ptr('0x719724').readU32()]); } });
Interceptor.attach(ptr('0x52cc00'), { onEnter() {
  try {
    const t = this.context.ecx, sp = this.context.esp;
    const a1 = sp.add(4).readPointer();
    const t1i = a1.add(0x60).readS32(), t1f = f(a1.add(0x64));
    const t2i = sp.add(8).readS32(), t2f = f(sp.add(12));
    const ba = t.add(0x20).readPointer(), bb = t.add(0x24).readPointer();
    if (first === null) first = t.toString();
    if (nfull < DFULL) {
      nfull++;
      learn(ba); learn(bb);
      push(['D', ++seq, t.toString(), ba.toString(), bb.toString(), a1.toString(), t1i, t1f, t2i, t2f, floats(t, 0x50)]);
    } else {
      push(['D', ++seq, t.toString(), t1i, t1f, t2i, t2f]);
    }
    if (t.toString() === first && !bb.isNull()) {
      push(state(bb));
      for (const [, p] of chassis) push(state(p));
    }
  } catch (err) { push(['E', ++seq, String(err)]); }
} });
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const wrap = this.context.ecx.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    if (!chassis.has(body.toString())) { chassis.set(body.toString(), body); learn(body); }
  } catch (err) { push(['E', ++seq, String(err)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
