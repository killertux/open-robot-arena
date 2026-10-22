// Frida agent: Havok's deactivation in the ORIGINAL (docs/29 section 12t).
// Two deactivators run (recovered statically, this agent confirms them):
//   * every robot's bodies belong to a system (vtable 0x6ac2a8, built by
//     0x533c80 from 0x5236e0 with its thresholds scaled by 10, 0x533db0); its
//     step (0x533fd0) calls 0x534050, which checks every 0.2 s (short) and
//     every 10 s (long) whether all its bodies are still (0x522cb0) and then
//     deactivates them all (vfunc +0x14 = 0x531e40 setActive(0, 1)); while
//     frozen it skips its actions and wakes them all once any is active.
//   * the world's deactivator (0x53cad0(5, 0.1) in the world, listener
//     0x53cc00) does the same per entity for entities in no system.
//   * the collision detector wakes an inactive movable body in a contact
//     (0x548da0 -> 0x548ef0).
// Rows (only while armed, besides P):
//   T  [T,seq,n]                      a stepTo entry (n counted from arm)
//   R  [R,seq]                        resolver entry (0x53ed80)
//   N  [N,seq]                        detection entry (0x542810)
//   Q  [Q,seq]                        actions entry (0x52a990)
//   B  [B,seq,body,flags,com(3),q(4),v(3),w(3)]  every watched body at each T
//   G  [G,seq,sys,ti,tf,active68,[t54,t58],[t60,t64],thr(4),bodies,ret]  0x534050
//   C  [C,seq,body,ti,tf,short,long,params,ret,com(3),q(4),sref(7),lref(7),r2,e20]
//   A  [A,seq,body,active,notify,ret,flagsBefore]  0x531e40
//   W  [W,seq,body,ret]               0x548ef0 (activate from the detector)
//   D  [D,seq,ti,tf,active,ts,tl]     0x53cc00 (the world's deactivator)
//   Y  [Y,seq,sys,[bodies],[actions vtables]]  a system seen for the first time
'use strict';
let seq = 0, rows = [], armed = 0, n = 0;
function push(r) { rows.push(r); if (rows.length >= 512) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const q4 = p => [f(p), f(p.add(4)), f(p.add(8)), f(p.add(12))];
const list = (p) => { const b = p.readPointer(), e = p.add(4).readPointer(), o = [];
  if (b.isNull()) return o; for (let r = b; r.compare(e) < 0 && o.length < 64; r = r.add(4)) o.push(r.readPointer()); return o; };
const systems = new Map(), watch = new Map();
function learnSystem(s) {
  const k = s.toString();
  if (systems.has(k)) return;
  const bodies = list(s.add(0x44)), acts = list(s.add(0x34));
  systems.set(k, s);
  bodies.forEach(b => watch.set(b.toString(), b));
  push(['Y', seq, k, bodies.map(b => [b.toString(), f(b.add(0x190)), vec(b.add(0x30)), b.add(0x20).readS32()]),
        acts.map(a => [a.toString(), a.readPointer().toString()])]);
}
Interceptor.attach(ptr('0x52aa60'), { onEnter() {
  if (!armed) return;
  push(['T', ++seq, n++]);
  for (const b of watch.values())
    push(['B', seq, b.toString(), b.add(0x1c).readU32(), vec(b.add(0x30)), q4(b.add(0x40)), vec(b.add(0x54)), vec(b.add(0x64))]);
  if (--armed === 0) flush();
} });
Interceptor.attach(ptr('0x53ed80'), { onEnter() { if (armed) push(['R', ++seq]); } });
Interceptor.attach(ptr('0x542810'), { onEnter() { if (armed) push(['N', ++seq]); } });
Interceptor.attach(ptr('0x52a990'), { onEnter() { if (armed) push(['Q', ++seq]); } });
Interceptor.attach(ptr('0x534050'), {
  onEnter() {
    const s = this.context.ecx;
    learnSystem(s);
    if (!armed) return;
    const sp = this.context.esp;
    this.r = ['G', ++seq, s.toString(), sp.add(4).readS32(), f(sp.add(8)), s.add(0x68).readU8(),
              [s.add(0x54).readS32(), f(s.add(0x58))], [s.add(0x60).readS32(), f(s.add(0x64))],
              [f(s.add(0x20)), f(s.add(0x24)), f(s.add(0x28)), f(s.add(0x2c))], list(s.add(0x44)).length];
  },
  onLeave(ret) { if (this.r) { this.r.push(ret.toInt32() & 0xff); push(this.r); } },
});
Interceptor.attach(ptr('0x522cb0'), {
  onEnter() {
    if (!armed) return;
    const b = this.context.ecx, sp = this.context.esp;
    const pr = sp.add(20).readPointer();
    this.r = ['C', ++seq, b.toString(), sp.add(4).readS32(), f(sp.add(8)), sp.add(12).readU8(), sp.add(16).readU8(),
              pr.isNull() ? null : [f(pr), f(pr.add(4)), f(pr.add(8)), f(pr.add(12))]];
    this.b = b;
  },
  onLeave(ret) {
    if (!this.r) return;
    const b = this.b;
    this.r.push(ret.toInt32() & 0xff, vec(b.add(0x30)), q4(b.add(0x40)),
                vec(b.add(0x220)).concat(q4(b.add(0x230))), vec(b.add(0x1fc)).concat(q4(b.add(0x20c))),
                f(b.add(0x244)), b.add(0x20).readS32());
    push(this.r);
  },
});
Interceptor.attach(ptr('0x531e40'), { onEnter() {
  if (!armed) return;
  const sp = this.context.esp, b = this.context.ecx;
  push(['A', ++seq, b.toString(), sp.add(4).readU8(), sp.add(8).readU8(), this.returnAddress.toString(), b.add(0x1c).readU32()]);
} });
Interceptor.attach(ptr('0x548ef0'), { onEnter() {
  if (!armed) return;
  push(['W', ++seq, this.context.esp.add(4).readPointer().toString(), this.returnAddress.toString()]);
} });
Interceptor.attach(ptr('0x53cc00'), { onEnter() {
  if (!armed) return;
  const sp = this.context.esp, d = this.context.ecx;
  push(['D', ++seq, sp.add(8).readS32(), f(sp.add(12)), d.add(0x38).readU8(), [f(d.add(0x20)), f(d.add(0x24))],
        [d.add(0x28).readS32(), f(d.add(0x2c)), d.add(0x30).readS32(), f(d.add(0x34))]]);
} });

rpc.exports = {
  flush() { flush(); return seq; },
  arm(steps) { armed = steps; n = 0; return true; },
  busy() { return armed > 0; },
  systems() { return Array.from(systems.keys()); },
  watchBody(s) { const p = ptr(s); watch.set(p.toString(), p); return true; },
  bodies() { return Array.from(watch.values()).map(b => [b.toString(), f(b.add(0x190)), vec(b.add(0x30)), b.add(0x20).readS32(), b.add(0x1c).readU32()]); },
  noImmobility() {
    const p = ptr('0x6a57f0');
    Memory.protect(p, 4, 'rw-');
    const was = p.readFloat();
    p.writeFloat(1e9);
    return was;
  },
};
setInterval(flush, 300);
