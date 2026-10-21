// Frida agent: the SmartZone events the ORIGINAL hands the shipped AIs, with
// both chassis where they are, and who calls the collision-event filter
// (docs/29 section 12n).  Function-entry hooks only.
//   A  [A,seq,h,rollbacks]   0x52a990 actions entry: one per accepted step piece
//   Z  [Z,seq,ai,[a1,a2,a3,a4],[chassis com...],[return addresses]]
//      0x4026b0, the C++ SmartZoneEvent thunk (`jmp [vtable+0x48]`) that
//      RegisterSmartZone (0x401340) binds to each zone collidable; its four
//      stack arguments as the Python side receives them (direction, id,
//      robot, chassis); the chassis seen by a MagicAction lately (an accurate
//      backtrace here crashes the game)
//   Y  [Y,seq,ai,[a1..a4]]   0x4c3210, the Python dispatch of SmartZoneEvent
//   V  [V,seq,[return addresses]]  0x52d4f0, the event filter (first MAX_V calls)
//   G  [G,seq,chassis]       0x4856f0 MagicAction apply, once per new chassis
//   P  [P,seq,leaving,[collA,tagA,phantomA],[collB,tagB,phantomB]]  0x4a4780, the
//      zone listener, for every phantom-pair event (type DAT_006abf6c): the
//      pair's collidables with their tag (+0x54) and phantom flag (+0x58)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const chassis = new Map();
const MAX_V = (typeof MAX_FILTER === 'number') ? MAX_FILTER : 400;
let nV = 0;
function bt(ctx, kind) {
  try { return Thread.backtrace(ctx, kind || Backtracer.FUZZY).slice(0, 12).map(a => a.toString()); }
  catch (e) { return [String(e)]; }
}
const seen = new Map();
function coms() {
  const o = [];
  for (const [k, p] of chassis) {
    if (seq - (seen.get(k) || 0) > 400) continue;
    try { o.push([k, vec(p.add(0x30))]); } catch (e) {}
  }
  return o;
}
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', ++seq, f(this.context.esp.add(4)), ptr('0x719724').readU32()]); } });
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const wrap = this.context.ecx.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    const k = body.toString();
    if (!chassis.has(k)) { chassis.set(k, body); push(['G', ++seq, k]); }
    seen.set(k, seq);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x4026b0'), { onEnter() {
  try {
    const sp = this.context.esp, a = [];
    for (let i = 1; i <= 4; i++) a.push(sp.add(4 * i).readS32());
    push(['Z', ++seq, this.context.ecx.toString(), a, coms(), bt(this.context)]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x4c3210'), { onEnter() {
  try {
    const sp = this.context.esp, a = [];
    for (let i = 1; i <= 4; i++) a.push(sp.add(4 * i).readS32());
    push(['Y', ++seq, this.context.ecx.toString(), a]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x52d4f0'), { onEnter() {
  if (nV >= MAX_V) return;
  nV++;
  push(['V', ++seq, bt(this.context)]);
} });
const PAIR = ptr('0x6abf6c').readU32();
Interceptor.attach(ptr('0x4a4780'), { onEnter() {
  try {
    const ev = this.context.esp.add(4).readPointer();
    if (ev.add(8).readU32() !== PAIR) return;
    const side = c => c.isNull() ? null : [c.toString(), c.add(0x54).readU32(), c.add(0x58).readU8()];
    push(['P', ++seq, ev.add(0x14).readU32(), side(ev.add(0xc).readPointer()), side(ev.add(0x10).readPointer())]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
