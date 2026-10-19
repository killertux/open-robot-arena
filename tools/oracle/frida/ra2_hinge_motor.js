// Frida agent: Havok hinge motors of the ORIGINAL (docs/29 section 10): C rows at the
// HingeController set-motor 0x4815c0 (argument layout unconfirmed: these rows errored), H rows
// at the hinge atom setup 0x535a50 while the motor is on: bodies, velocity, masses, w, com.
// Used with Arenas.currentArena.Hammer1.Smash() on the Octagon (hinge-octagon__live__000).
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 128) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
// HingeController set-motor: thiscall(ctl, vel, torque)
Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  try {
    const c = this.context.ecx, sp = this.context.esp, h = c.add(0x58).readPointer();
    push(['C', ++seq, c.toString(), h.toString(), f(sp.add(4)), f(sp.add(8)), c.add(0x7c).readS32(), f(c.add(0xbc)),
          vec(h.add(0x50)), vec(h.add(0x60)), f(c.add(0x70)), f(c.add(0x74))]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
// hinge atom setup: bodies and their angular velocity while the motor is on
let n = 0;
Interceptor.attach(ptr('0x535a50'), { onEnter() {
  try {
    const a = this.context.ecx;
    if (!a.add(0x81).readU8()) return;
    if ((n++ % 4) !== 0) return;
    const bA = a.add(0xc).readPointer(), bB = a.add(0x10).readPointer();
    push(['H', ++seq, a.toString(), bA.toString(), bB.toString(), f(a.add(0x94)), f(bA.add(0x190)), f(bB.add(0x190)),
          vec(bA.add(0x64)), vec(bB.add(0x64)), vec(bA.add(0x30)), vec(bB.add(0x30))]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
