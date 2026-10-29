// Frida agent: when a burst motor's fire reaches the hinge's solver atom
// (docs/29 section 12f).  Rows (n = stepTo calls so far, 0x52aa60):
//   S  [S, n]                                   stepTo entry
//   A  [A, n, h]                                actions of an accepted step piece (0x52a990)
//   F  [F, n, burstMotor]                       a burst motor fires (0x446810)
//   K  [K, n, controller, locked]               the hinge controller's Lock (0x480e20)
//   C  [C, n, controller]                       its update (0x481450), near a fire only
//   H  [H, n, atom, motor, limited, velocity, maxTorque, dt, targetBefore, targetAfter,
//       theta, lower, upper, wA, wB]            the hinge atom's setup (0x535a50) for
//                                               atoms with a limit or a motor, near a fire
'use strict';
let n = 0, lastFire = -1000, rows = [];
function push(r) { rows.push(r); if (rows.length >= 128) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const near = () => n - lastFire < 60;
Interceptor.attach(ptr('0x52aa60'), { onEnter() { n++; if (near()) push(['S', n]); } });
Interceptor.attach(ptr('0x52a990'), { onEnter() { if (near()) push(['A', n, f(this.context.esp.add(4))]); } });
Interceptor.attach(ptr('0x446810'), { onEnter() { lastFire = n; push(['F', n, this.context.ecx.toString()]); } });
Interceptor.attach(ptr('0x480e20'), { onEnter() {
  push(['K', n, this.context.ecx.toString(), this.context.esp.add(4).readU8()]); } });
Interceptor.attach(ptr('0x481450'), { onEnter() { if (near()) push(['C', n, this.context.ecx.toString()]); } });
let cur = null;
Interceptor.attach(ptr('0x535a50'), {
  onEnter() {
    cur = null;
    if (!near()) return;
    const a = this.context.ecx, info = this.context.esp.add(4).readPointer();
    const motor = a.add(0x81).readU8(), lim = a.add(0x80).readU8();
    if (!motor && !lim) return;
    const bA = a.add(0xc).readPointer(), bB = a.add(0x10).readPointer();
    cur = { a: a, motor: motor, lim: lim, vel: f(a.add(0x94)), maxT: f(a.add(0x90)), dt: f(info.add(4)),
            pre: f(a.add(0x98)), lo: f(a.add(0x84)), hi: f(a.add(0x88)), theta: NaN,
            wA: vec(bA.add(0x64)), wB: vec(bB.add(0x64)) };
    this.c = cur;
  },
  onLeave() {
    const c = this.c; if (!c) return;
    push(['H', n, c.a.toString(), c.motor, c.lim, c.vel, c.maxT, c.dt, c.pre, f(c.a.add(0x98)), c.theta,
          c.lo, c.hi, c.wA, c.wB]);
    cur = null;
  } });
// theta from the caller's stack in the call 0x535c51 -> 0x5363b0 (function entries only:
// mid-function hooks in the x87 code crash the game).
Interceptor.attach(ptr('0x5363b0'), { onEnter() {
  if (cur && this.returnAddress.equals(ptr('0x535c56'))) cur.theta = f(this.context.esp.add(0x14)); } });
rpc.exports = { flush() { flush(); return n; } };
setInterval(flush, 300);
