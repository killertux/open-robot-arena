// Frida agent: controlled two-robot placements in the ORIGINAL (docs/29 section 12s).
// The placement of ra2_tipab.js for any set of bodies, every body's target
// given whole (the capture script computes the poses), plus what the two
// robots do to each other step by step:
//   arm(plan): plan = {bodies: [ptr...], hold: n, lift: m, rest: [[com, quat], ...],
//     place: [[com(3), quat(4) w x y z, vel(3), angvel(3)], ...] per body, steps: n}
//   For `hold` substeps every body is written at its rest pose `lift` m higher
//   with no velocity; at the next substep every body is written at its
//   `place` state; then the world is left alone for `steps` substeps. Each
//   write is followed by the body's own sync (0x520750).
// Rows (only while a plan runs, besides P):
//   R  [R,seq,tol,contacts]  0x53ed80 entry, as ra2_contacts.js
//   B  [B,seq,body,mass,com(3),vel(3),angvel(3),rot(9),flags]  every plan body, once per R
//      (flags: the body's +0x1c, bit 1 = active, docs/29 section 12j)
//   V  [V,seq,body,vel(3),angvel(3)]  every plan body as the resolver leaves it
//      (0x53ed80 exit): with B, each body's contact impulse of the piece
//   A  [A,seq,h,rollbacks]  0x52a990 entry
//   G  [G,seq,chassis,[[thrust,touching],...]]  0x4856f0 entry, every MagicAction
//   M  [M,seq,motor,speed,torque]  0x4815c0 SetPowerSettings of a hinge
//      controller (when it changes): a burst motor's fire and cock
//   F  [F,seq,controller,force]  0x4b51b0: a burst piston's one-shot push
//   TM/TP [TM,seq,dt,this]  0x446670 / 0x448650: a burst motor's / piston's tick
//   X  [X,seq,phase,[[body, com(3), quat(4), vel(3), angvel(3)],...]]  a write
//      ('hold' first and last, 'place'; 'done' when the run is over)
//   T  [T,seq,step]  a stepTo entry (step counted from the place)
//   P  [P,seq,body,[floats 0..0x200]]  once per body
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const wv = (p, v) => { p.writeFloat(v[0]); p.add(4).writeFloat(v[1]); p.add(8).writeFloat(v[2]); };
const floats = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
const known = new Map();
function learn(p) {
  const k = p.toString();
  if (known.has(k)) return;
  known.set(k, p);
  push(['P', seq, k, floats(p, 0x200)]);
}
const sync = new NativeFunction(ptr('0x520750'), 'void', ['pointer'], 'thiscall');
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

function readState(p) {
  return { com: vec(p.add(0x30)), q: [f(p.add(0x40)), f(p.add(0x44)), f(p.add(0x48)), f(p.add(0x4c))],
           v: vec(p.add(0x54)), w: vec(p.add(0x64)) };
}
function writeState(p, com, q, v, w) {
  wv(p.add(0x30), com);
  p.add(0x40).writeFloat(q[0]); p.add(0x44).writeFloat(q[1]);
  p.add(0x48).writeFloat(q[2]); p.add(0x4c).writeFloat(q[3]);
  wv(p.add(0x54), v);
  wv(p.add(0x64), w);
  sync(p);
}
function dumpX(phase, bodies) {
  push(['X', ++seq, phase, bodies.map(p => {
    const s = readState(p);
    return [p.toString(), s.com, s.q, s.v, s.w];
  })]);
}

let plan = null;
const active = () => plan !== null;
Interceptor.attach(ptr('0x52aa60'), { onEnter() {
  if (!plan) return;
  try {
    const P = plan;
    if (P.k === undefined) P.k = 0;
    if (P.k < P.hold) {
      const up = [0, P.lift, 0];
      P.ptrs.forEach((p, i) => writeState(p, add(P.rest[i][0], up), P.rest[i][1], [0, 0, 0], [0, 0, 0]));
      if (P.k === 0 || P.k === P.hold - 1) dumpX('hold', P.ptrs);
    } else if (P.k === P.hold) {
      P.ptrs.forEach((p, i) => { const s = P.place[i]; writeState(p, s[0], s[1], s[2], s[3]); });
      dumpX('place', P.ptrs);
    }
    const n = P.k - P.hold;
    push(['T', ++seq, n]);
    P.k++;
    if (n >= P.steps) { dumpX('done', P.ptrs); plan = null; flush(); }
  } catch (err) { push(['E', ++seq, String(err)]); plan = null; }
} });

function bodyRow(tag, p) {
  const xf = p.add(0xcc), rot = [];
  for (let i = 0; i < 9; i++) rot.push(f(xf.add(4 * i)));
  push([tag, seq, p.toString(), f(p.add(0x190)), vec(p.add(0x30)), vec(p.add(0x54)), vec(p.add(0x64)), rot,
        p.add(0x1c).readU32()]);
}
Interceptor.attach(ptr('0x53ed80'), {
  onEnter() {
    if (!active()) return;
    try {
      const sp = this.context.esp;
      const list = sp.add(4).readPointer();
      const tol = f(sp.add(8));
      const b = list.add(4).readPointer(), e = list.add(8).readPointer();
      const out = [];
      if (!b.isNull()) {
        let n = 0;
        for (let r = b; r.compare(e) < 0 && n < 400; r = r.add(0x74), n++) {
          const ca = r.add(8).readPointer(), cb = r.add(0xc).readPointer();
          const ba = ca.add(0x1c).readPointer(), bb = cb.add(0x1c).readPointer();
          for (const p of [ba, bb]) if (!known.has(p.toString()) && f(p.add(0x190)) > 0) learn(p);
          out.push([ba.toString(), bb.toString(), vec(r.add(0x10)), vec(r.add(0x20)), f(r.add(0x54)),
                    f(r.add(0x58)), f(r.add(0x5c)), f(r.add(0x60)), f(r.add(0x64)), ca.toString(), cb.toString()]);
        }
      }
      push(['R', ++seq, tol, out]);
      for (const p of plan.ptrs) bodyRow('B', p);
      this.on = true;
    } catch (err) { push(['E', ++seq, String(err)]); }
  },
  onLeave() {
    if (!this.on || !plan) return;
    try {
      for (const p of plan.ptrs) push(['V', seq, p.toString(), vec(p.add(0x54)), vec(p.add(0x64))]);
    } catch (err) { push(['E', ++seq, String(err)]); }
  },
});
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  if (active()) push(['A', ++seq, f(this.context.esp.add(4)), ptr('0x719724').readU32()]); } });
const chassis = new Map();
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const t = this.context.ecx;
    const wrap = t.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    learn(body);
    chassis.set(body.toString(), body);
    if (active()) {
      const b = t.add(0x24).readPointer(), e = t.add(0x28).readPointer();
      const ws = [];
      for (let r = b; r.compare(e) < 0; r = r.add(0x50)) ws.push([r.add(4).readS32(), r.add(8).readU8()]);
      push(['G', ++seq, body.toString(), ws]);
    }
  } catch (err) { push(['E', ++seq, String(err)]); }
} });
const last = {};
Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  if (!active()) return;
  const m = this.context.ecx.toString(), sp = this.context.esp, v = sp.add(4).readFloat(), t = sp.add(8).readFloat();
  if (last[m] && last[m][0] === v && last[m][1] === t) return;
  last[m] = [v, t];
  push(['M', ++seq, m, v, t]);
} });
Interceptor.attach(ptr('0x4b51b0'), { onEnter() {
  if (active()) push(['F', ++seq, this.context.ecx.toString(), this.context.esp.add(4).readFloat()]); } });
Interceptor.attach(ptr('0x446670'), { onEnter() {
  if (active()) push(['TM', ++seq, this.context.esp.add(4).readFloat(), this.context.ecx.toString()]); } });
Interceptor.attach(ptr('0x448650'), { onEnter() {
  if (active()) push(['TP', ++seq, this.context.esp.add(4).readFloat(), this.context.ecx.toString()]); } });

rpc.exports = {
  flush() { flush(); return seq; },
  bodies() { return Array.from(known.values()).map(p => [p.toString(), f(p.add(0x190)), vec(p.add(0x30))]); },
  chassis() { return Array.from(chassis.values()).map(p => [p.toString(), f(p.add(0x190)), vec(p.add(0x30))]); },
  // Every rigid body (vtable 0x6abe04) between a and b: [ptr, mass, com].
  scan(a, b) {
    const o = [];
    for (let p = ptr(a); p.compare(ptr(b)) < 0; p = p.add(0x20)) {
      try { if (p.readU32() === 0x6abe04) o.push([p.toString(), f(p.add(0x190)), vec(p.add(0x30))]); } catch (e) {}
    }
    return o;
  },
  // The bodies' states now: [[com, quat w x y z, vel, angvel], ...].
  states(list) { return list.map(s => { const r = readState(ptr(s)); return [r.com, r.q, r.v, r.w]; }); },
  learnAll(list) { list.forEach(s => learn(ptr(s))); return true; },
  arm(p) {
    p.ptrs = p.bodies.map(s => ptr(s));
    for (const k of Object.keys(last)) delete last[k];
    plan = p;
    return true;
  },
  busy() { return plan !== null; },
  step() { return plan && plan.k !== undefined ? plan.k - plan.hold : -1000; },
  noImmobility() {
    const p = ptr('0x6a57f0');
    Memory.protect(p, 4, 'rw-');
    const was = p.readFloat();
    p.writeFloat(1e9);
    return was;
  },
};
setInterval(flush, 300);
