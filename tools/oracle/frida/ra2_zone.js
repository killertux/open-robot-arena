// Frida agent: what a robot's SmartZone holds in the ORIGINAL, as Havok's
// phantom list has it, with the chassis poses to test the geometry offline
// (docs/29 section 12i).  Function-entry hooks only.
//
// GetZoneContents (0x4014f0, thiscall on the robot, arg 2 the zone's id) copies
// the world's phantom pair list (0x4ae1f0: *(*(*(0x714670) + 0x20) + 0x18) +
// 0x100, begin +4 / end +8, pairs of collidables) and keeps each pair holding
// the zone's collidable, tagged by the other collidable's +0x54
// (robot << 4 | chassis).  Rows:
//   Z  [Z,seq,robot,zoneArg]              a GetZoneContents call
//   L  [L,seq,[[cA,cB,clsA,clsB,tagA,tagB,phA,phB,bodyA,bodyB],...]]
//        the phantom list as it is at that call, once per world step
//        (collidable +0x18 shape class, +0x54 tag, +0x58 phantom byte, +0x1c body)
//   B  [B,seq,body,mass,com(3),rot(9)]    every chassis body (from MagicAction)
//        and every body in the list, once per L
//   A  [A,seq,h,rollbacks]                0x52a990 actions entry, one per step piece
'use strict';
let seq = 0, rows = [], stepSeen = -1, step = 0;
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const chassis = new Map();

function pose(k, p) {
  const xf = p.add(0xcc), rot = [];
  for (let i = 0; i < 9; i++) rot.push(f(xf.add(4 * i)));
  push(['B', seq, k, f(p.add(0x190)), vec(p.add(0x30)), rot]);
}

Interceptor.attach(ptr('0x4014f0'), { onEnter() {
  try {
    push(['Z', ++seq, this.context.ecx.toString(), this.context.esp.add(8).readU32()]);
    if (stepSeen === step) return;
    stepSeen = step;
    const world = ptr('0x714670').readPointer().add(0x20).readPointer();
    const det = world.add(0x18).readPointer();
    const b = det.add(0x104).readPointer(), e = det.add(0x108).readPointer();
    const out = [], bodies = new Map();
    if (!b.isNull()) {
      for (let r = b; r.compare(e) < 0 && out.length < 200; r = r.add(8)) {
        const ca = r.readPointer(), cb = r.add(4).readPointer();
        const ba = ca.add(0x1c).readPointer(), bb = cb.add(0x1c).readPointer();
        out.push([ca.toString(), cb.toString(), ca.add(0x18).readU32(), cb.add(0x18).readU32(),
                  ca.add(0x54).readU32(), cb.add(0x54).readU32(), ca.add(0x58).readU8(), cb.add(0x58).readU8(),
                  ba.toString(), bb.toString()]);
        bodies.set(ba.toString(), ba); bodies.set(bb.toString(), bb);
      }
    }
    push(['L', seq, out]);
    for (const [k, p] of chassis) bodies.set(k, p);
    for (const [k, p] of bodies) if (!p.isNull()) pose(k, p);
  } catch (err) { push(['E', ++seq, String(err)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  step++;
  push(['A', ++seq, f(this.context.esp.add(4)), ptr('0x719724').readU32()]); } });
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const wrap = this.context.ecx.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    chassis.set(body.toString(), body);
  } catch (err) { push(['E', ++seq, String(err)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
