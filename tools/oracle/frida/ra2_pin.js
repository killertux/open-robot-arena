// Frida agent: hold chosen rigid bodies of the ORIGINAL at chosen poses (docs/37 section 6, the flame's hit rate
// against distance).  Every Havok substep (0x52aa60 entry) each pinned body is written (position, orientation,
// zero velocities) and synced (0x520750), as ra2_twobody.js does for its placements.
//   chassis()      [[ptr, mass, com]] the robot chassis bodies seen (0x4856f0, the MagicAction)
//   scan(a, b)     [[ptr, mass, com]] every rigid body (vtable 0x6abe04) between the addresses
//   states(list)   [[com, quat w x y z]]
//   pin(list)      list = [[ptr, com, quat]]: write them every substep from now on
//   unpin()        stop
//   noImmobility() raise the immobility limit (0x6a57f0) so a held robot is not counted out
'use strict';
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const wv = (p, v) => { p.writeFloat(v[0]); p.add(4).writeFloat(v[1]); p.add(8).writeFloat(v[2]); };
const sync = new NativeFunction(ptr('0x520750'), 'void', ['pointer'], 'thiscall');
function writeState(p, com, q) {
  wv(p.add(0x30), com);
  p.add(0x40).writeFloat(q[0]); p.add(0x44).writeFloat(q[1]); p.add(0x48).writeFloat(q[2]); p.add(0x4c).writeFloat(q[3]);
  wv(p.add(0x54), [0, 0, 0]);
  wv(p.add(0x64), [0, 0, 0]);
  sync(p);
}
let pins = null;
let writes = 0;
Interceptor.attach(ptr('0x52aa60'), { onEnter() {
  if (!pins) return;
  try { for (const [p, com, q] of pins) writeState(p, com, q); writes++; } catch (e) { send({ ev: 'err', e: String(e) }); }
} });
const chassis = new Map();
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const t = this.context.ecx;
    const wrap = t.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    chassis.set(body.toString(), body);
  } catch (e) {}
} });
rpc.exports = {
  chassis() { return Array.from(chassis.values()).map(p => [p.toString(), f(p.add(0x190)), vec(p.add(0x30))]); },
  scan(a, b) {
    const o = [];
    for (let p = ptr(a); p.compare(ptr(b)) < 0; p = p.add(0x20)) {
      try { if (p.readU32() === 0x6abe04) o.push([p.toString(), f(p.add(0x190)), vec(p.add(0x30))]); } catch (e) {}
    }
    return o;
  },
  states(list) { return list.map(s => { const p = ptr(s); return [vec(p.add(0x30)), [f(p.add(0x40)), f(p.add(0x44)), f(p.add(0x48)), f(p.add(0x4c))]]; }); },
  pin(list) { pins = list.map(([s, c, q]) => [ptr(s), c, q]); writes = 0; return true; },
  unpin() { pins = null; return writes; },
  writes() { return writes; },
  noImmobility() { const p = ptr('0x6a57f0'); Memory.protect(p, 4, 'rw-'); const was = p.readFloat(); p.writeFloat(1e9); return was; },
};
