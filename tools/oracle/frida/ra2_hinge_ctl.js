// Frida agent: the original's hinge controller (vtable 0x6a7e44) and how its power settings
// reach the Havok motor (docs/29 section 12).
//   P rows: SetPowerSettings 0x4810e0 (ctl, speed, torque, caller)
//   L rows: the load cap 0x480510 (ctl, caller, +0xb0..+0xbc before and after)
//   M rows: the motor set 0x4815c0 (atom+0x90, velocity, max torque, ctl's +0x7c) on change
//   W rows: the prismatic/wheel slot-10 functions 0x4b4c50 / 0x508860 (ctl, caller)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 128) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const pw = c => [f(c.add(0xb0)), f(c.add(0xb4)), f(c.add(0xb8)), f(c.add(0xbc))];
function bt(ctx) {
  try { return Thread.backtrace(ctx, Backtracer.FUZZY).slice(0, 6).map(a => a.toString()); }
  catch (e) { return []; }
}
Interceptor.attach(ptr('0x4810e0'), { onEnter() {
  try {
    const c = this.context.ecx, sp = this.context.esp;
    push(['P', ++seq, Date.now(), c.toString(), f(sp.add(4)), f(sp.add(8)), this.returnAddress.toString(), c.readPointer().toString()]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x480510'), {
  onEnter() {
    try { this.c = this.context.ecx; this.b = pw(this.c); this.ra = this.returnAddress.toString(); this.bt = bt(this.context); }
    catch (e) { push(['E', ++seq, String(e)]); }
  },
  onLeave() {
    try {
      const a = this.c.add(0x58).readPointer();
      const atom = [];
      for (let o = 0x30; o <= 0x90; o += 0x10) atom.push(vec(a.add(o)));
      push(['L', ++seq, Date.now(), this.c.toString(), this.ra, this.b, pw(this.c), this.bt, atom,
            this.c.add(0x5c).readPointer().toString(), this.c.add(0x60).readPointer().toString()]);
    }
    catch (e) { push(['E', ++seq, String(e)]); }
  } });
for (const a of ['0x4b4c50', '0x508860']) {
  Interceptor.attach(ptr(a), { onEnter() {
    try { push(['W', ++seq, a, this.context.ecx.toString(), this.returnAddress.toString()]); }
    catch (e) { push(['E', ++seq, String(e)]); }
  } });
}
const last = {};
Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  try {
    const m = this.context.ecx, sp = this.context.esp;
    const v = f(sp.add(4)), t = f(sp.add(8));
    const k = m.toString();
    if (last[k] && last[k][0] === v && last[k][1] === t) return;
    last[k] = [v, t];
    push(['M', ++seq, Date.now(), k, v, t, this.returnAddress.toString()]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
