// Frida agent: how a match turns a robot onto its start point in the ORIGINAL
// (docs/29 section 12w). The robots are built at the arena's origin; at the
// countdown each is set on its start point (0x4dcff0: the chassis turned, every
// other body only moved), the world steps while the joints pull the other
// bodies round, and the robot is lowered (0x418b90) and held until the bell.
// Loaded before Start Game (capture_rest.py WATCH_AGENT=ra2_spinup.js); armed
// from the first robot system's step for MAX_STEPS substeps.
//   Y  [Y,n,sys,[[body,mass,com(3),flags],...]]   a robot system seen first
//   T  [T,n]                                       a stepTo entry (0x52aa60)
//   B  [B,n,body,flags,com(3),q(4),v(3),w(3)]     every watched body at each T
//   P  [P,n,'enter'|'leave',this,ret]              0x4dcff0 (start point)
//   L  [L,n,'enter'|'leave',this,ret]              0x418b90 (lowering)
//   H  [H,n,atom,bodyA,bodyB,motor,limited,target,targetVel,maxTorque,lo,hi]
//      0x535a50 (hinge set-up) on a watched body, whenever these change
//   J  [J,n,atom,bodyA,bodyB,atom floats 0..0xa0]  the atom's first set-up, and every
//      set-up while a robot is being placed
//   (Y rows also carry each body's local inertia +0x19c and its first 0x200 bytes)
'use strict';
const MAX_STEPS = 6000;
let n = 0, armed = false, rows = [], placing = false;
function push(r) { rows.push(r); if (rows.length >= 512) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const floats = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
const seenAtom = new Set();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const q4 = p => [f(p), f(p.add(4)), f(p.add(8)), f(p.add(12))];
const list = (p) => { const b = p.readPointer(), e = p.add(4).readPointer(), o = [];
  if (b.isNull()) return o; for (let r = b; r.compare(e) < 0 && o.length < 64; r = r.add(4)) o.push(r.readPointer()); return o; };
const systems = new Set(), watch = new Map(), hinge = new Map();
Interceptor.attach(ptr('0x534050'), { onEnter() {
  const s = this.context.ecx;
  try {
    const bodies = list(s.add(0x44));
    if (!bodies.some(b => f(b.add(0x190)) >= 20)) return;
    const fresh = bodies.filter(b => !watch.has(b.toString()));
    if (!systems.has(s.toString()) || fresh.length) {
      systems.add(s.toString());
      bodies.forEach(b => watch.set(b.toString(), b));
      armed = true;
      push(['Y', n, s.toString(), bodies.map(b => [b.toString(), f(b.add(0x190)), vec(b.add(0x30)), b.add(0x1c).readU32(),
            floats(b.add(0x19c), 0x30), floats(b.add(0x0), 0x200)])]);
    }
  } catch (e) {}
} });
Interceptor.attach(ptr('0x52aa60'), { onEnter() {
  if (!armed || n >= MAX_STEPS) return;
  push(['T', ++n]);
  for (const b of watch.values())
    push(['B', n, b.toString(), b.add(0x1c).readU32(), vec(b.add(0x30)), q4(b.add(0x40)), vec(b.add(0x54)), vec(b.add(0x64))]);
  if (n === MAX_STEPS) flush();
} });
for (const [addr, tag] of [['0x4dcff0', 'P'], ['0x418b90', 'L']]) {
  Interceptor.attach(ptr(addr), {
    onEnter() { this.t = this.context.ecx.toString(); if (tag === 'P') placing = true; push([tag, n, 'enter', this.t, this.returnAddress.toString()]); },
    onLeave() { if (tag === 'P') placing = false; push([tag, n, 'leave', this.t]); },
  });
}
Interceptor.attach(ptr('0x535a50'), { onEnter() {
  if (!armed || n >= MAX_STEPS) return;
  const a = this.context.ecx;
  const ba = a.add(0xc).readPointer().toString(), bb = a.add(0x10).readPointer().toString();
  if (!watch.has(ba) && !watch.has(bb)) return;
  const st = [a.add(0x81).readU8(), a.add(0x80).readU8(), f(a.add(0x98)), f(a.add(0x94)), f(a.add(0x90)), f(a.add(0x84)), f(a.add(0x88))];
  const k = a.toString(), key = JSON.stringify(st);
  if (!seenAtom.has(k) || placing) { seenAtom.add(k); push(['J', n, k, ba, bb, floats(a, 0xa0)]); }
  if (hinge.get(k) === key) return;
  hinge.set(k, key);
  push(['H', n, k, ba, bb].concat(st));
} });
setInterval(flush, 300);
rpc.exports = { flush() { flush(); return n; } };
