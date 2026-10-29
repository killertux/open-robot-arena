// Frida agent: the test the ORIGINAL lowers a robot onto its start point with
// (docs/29 section 12v). FUN_00418b90 moves the robot down 0.05 m at a time
// and after each move asks FUN_004adfd0(body, set) for every body of it; that
// runs FUN_00542a70(body, A, B, C) (the broadphase, then the agents of every
// pair with the body) and answers true when A (8-byte pairs, also gathered
// into `set`), B (0x74-byte contact records), C (8-byte pairs) or the
// detector's list FUN_0052e7d0 returns holds anything. Rows, only inside
// FUN_00418b90:
//   L  [L,seq,'enter'|'leave']                          FUN_00418b90
//   Q  [Q,seq,body,mass,centre(3),A,B,C]                 FUN_00542a70 leave; A/C:
//      [[collA, collB],...] with each collidable as [ptr, type(+0x18), body,
//      mass, +0x58 byte]; B: [[collA, collB, point(3), distance],...]
//   D  [D,seq,[[collA, collB],...]]                       FUN_0052e7d0's list as
//      FUN_004adfd0 reads it
//   R  [R,seq,ret]                                       FUN_004adfd0 leave
'use strict';
let seq = 0, rows = [], active = false;
function push(r) { rows.push(r); if (rows.length >= 64) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
function coll(c) {
  try {
    const b = c.add(0x1c).readPointer();
    return [c.toString(), c.add(0x18).readS32(), b.toString(), b.isNull() ? 0 : f(b.add(0x190)), c.add(0x58).readU8()];
  } catch (e) { return [c.toString()]; }
}
function pairs(list) {
  const b = list.add(4).readPointer(), e = list.add(8).readPointer(), o = [];
  if (b.isNull()) return o;
  for (let p = b; p.compare(e) < 0 && o.length < 32; p = p.add(8)) o.push([coll(p.readPointer()), coll(p.add(4).readPointer())]);
  return o;
}
function contacts(list) {
  const b = list.add(4).readPointer(), e = list.add(8).readPointer(), o = [];
  if (b.isNull()) return o;
  for (let p = b; p.compare(e) < 0 && o.length < 32; p = p.add(0x74))
    o.push([coll(p.add(8).readPointer()), coll(p.add(0xc).readPointer()), vec(p.add(0x20)), f(p.add(0x54))]);
  return o;
}
Interceptor.attach(ptr('0x418b90'), {
  onEnter() { active = true; push(['L', ++seq, 'enter']); },
  onLeave() { push(['L', ++seq, 'leave']); active = false; flush(); },
});
Interceptor.attach(ptr('0x542a70'), {
  onEnter() {
    if (!active) return;
    const sp = this.context.esp;
    this.a = [sp.add(4).readPointer(), sp.add(8).readPointer(), sp.add(12).readPointer(), sp.add(16).readPointer()];
  },
  onLeave() {
    if (!this.a) return;
    const [body, A, B, C] = this.a;
    try {
      push(['Q', ++seq, body.toString(), f(body.add(0x190)), vec(body.add(0x30)), pairs(A), contacts(B), pairs(C)]);
    } catch (e) { push(['E', ++seq, 'Q ' + String(e)]); }
  },
});
Interceptor.attach(ptr('0x52e7d0'), { onLeave(ret) {
  if (!active || this.returnAddress.compare(ptr('0x4ae094')) !== 0) return;
  try { push(['D', ++seq, pairs(ret)]); } catch (e) { push(['E', ++seq, 'D ' + String(e)]); }
} });
Interceptor.attach(ptr('0x4adfd0'), { onLeave(ret) {
  if (active) push(['R', ++seq, ret.toInt32() & 0xff]);
} });
setInterval(flush, 300);
rpc.exports = { flush() { flush(); return seq; } };
