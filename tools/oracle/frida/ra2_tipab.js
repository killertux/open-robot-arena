// Frida agent: controlled tip experiments in the ORIGINAL (docs/29 section 12r).
// ra2_contacts.js's rows, plus a placement plan run at the head of every
// World::stepTo (0x52aa60, one call per 1/60 s substep):
//   arm(plan): plan = {bodies: [ptr...] (chassis first), hold: n, lift: m,
//     axis: [x,y,z] in the chassis' frame, angle: rad, w: rad/s about the same
//     axis, v: [x,y,z] world, centre: [x,y,z] the chassis' centre of mass after
//     the turn (or dcentre: the same from where the chassis' centre was), steps: n, fire: substep or -1, piston: component ptr}
//   plan.extra (docs/29 12bd): [{bodies: [ptr...] (chassis first), axis, angle, centre (absolute) or
//     dcentre, w, v}]: more groups (another robot) held, turned about their own chassis' centre and
//     released at the same substep as the main group; their rest pose is taken once, at the first
//     plan that names them.
//   The bodies' centres (+0x30) and orientations (+0x40 w, +0x44 x y z) are
//   taken at the first substep of the first plan (or of one with reset: true)
//   and kept as the rest pose (or given: rest: [[com, quat], ...]; a Y row
//   prints the one used); for `hold` substeps every body is written back
//   `lift` m higher with no velocity (+0x54, +0x64), so no contact of the old
//   pose is left; then once turned rigidly about the chassis' centre by
//   `angle` about the axis, the chassis' centre at `centre`, each body moving
//   at v + w × r and spinning at w; then left alone for `steps` substeps.
//   Each write is followed by the body's own sync (0x520750: rotation matrix
//   +0xcc from the quaternion, +0xf0 from the centre, the world inverse
//   inertia +0x10c, the version count +0x25c).
// Rows:
//   R, B, A, P  as ra2_contacts.js (B only for the watched bodies once set);
//   G  as ra2_contacts.js, the first watched body's MagicAction only, while a plan runs
//   X  [X,seq,phase,[[body, com(3), quat(4), vel(3), angvel(3)],...]]  a write
//      (phase 'hold' first and last, 'place'); 'done' when the run is over
//   T  [T,seq,step]  a stepTo entry while a plan runs (step counted from the place)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const wv = (p, v) => { p.writeFloat(v[0]); p.add(4).writeFloat(v[1]); p.add(8).writeFloat(v[2]); };
const known = new Map();
let watch = null;
const floats = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
function learn(p) {
  const k = p.toString();
  if (known.has(k)) return;
  known.set(k, p);
  push(['P', seq, k, floats(p, 0x200)]);
}
const sync = new NativeFunction(ptr('0x520750'), 'void', ['pointer'], 'thiscall');

// Quaternions [w, x, y, z].
function qmul(a, b) {
  return [a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
          a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
          a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
          a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0]];
}
function qrot(q, v) {
  const p = qmul(qmul(q, [0, v[0], v[1], v[2]]), [q[0], -q[1], -q[2], -q[3]]);
  return [p[1], p[2], p[3]];
}
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const scale = (a, s) => [a[0] * s, a[1] * s, a[2] * s];

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

const R5 = x => Math.round(x * 1e5) / 1e5;
let wheelRows = false;
let plan = null, rest = null;
// FM [FM,seq,this] a burst motor fires (0x446810, the Scout's fork); BF [BF,seq,this] a burst piston fires (0x448750).
Interceptor.attach(ptr('0x446810'), { onEnter() { if (plan) push(['FM', ++seq, this.context.ecx.toString()]); } });
Interceptor.attach(ptr('0x448750'), { onEnter() { if (plan) push(['BF', ++seq, this.context.ecx.toString()]); } });
const restExtra = new Map();
const fire = new NativeFunction(ptr('0x448650'), 'void', ['pointer'], 'thiscall');
Interceptor.attach(ptr('0x52aa60'), { onEnter() {
  if (!plan) return;
  try {
    const P = plan;
    if (!P.snap) {
      // The rest pose is taken once, at the first plan, and kept for every
      // later one (a trial ends wherever the robot came to rest).
      if (P.rest) rest = P.rest.map(r => ({ com: r[0], q: r[1] }));
      if (!rest || P.reset) rest = P.ptrs.map(p => readState(p));
      if (P.k === undefined) push(['Y', ++seq, rest.map(r => [r.com, r.q])]);
      P.snap = rest;
      P.k = 0;
      if (P.dcentre) P.centre = add(P.snap[0].com, P.dcentre);
    }
    const c0 = P.snap[0].com;
    if (P.k < P.hold) {
      const up = [0, P.lift, 0];
      P.ptrs.forEach((p, i) => writeState(p, add(P.snap[i].com, up), P.snap[i].q, [0, 0, 0], [0, 0, 0]));
      if (P.k === 0 || P.k === P.hold - 1) dumpX('hold', P.ptrs);
      for (const G of (P.extra || [])) {
        G.ptrs.forEach((p, i) => writeState(p, add(G.snap[i].com, up), G.snap[i].q, [0, 0, 0], [0, 0, 0]));
        if (P.k === 0 || P.k === P.hold - 1) dumpX('hold', G.ptrs);
      }
    } else if (P.k === P.hold) {
      const axisW = qrot(P.snap[0].q, P.axis);
      const h = P.angle / 2, s = Math.sin(h);
      const Q = [Math.cos(h), axisW[0] * s, axisW[1] * s, axisW[2] * s];
      const w = scale(axisW, P.w);
      P.ptrs.forEach((p, i) => {
        const com = add(P.centre, qrot(Q, sub(P.snap[i].com, c0)));
        const v = add(P.v, cross(w, sub(com, P.centre)));
        writeState(p, com, qmul(Q, P.snap[i].q), v, w);
      });
      dumpX('place', P.ptrs);
      for (const G of (P.extra || [])) {
        const gc0 = G.snap[0].com;
        const gAxisW = qrot(G.snap[0].q, G.axis);
        const gh = G.angle / 2, gs = Math.sin(gh);
        const gQ = [Math.cos(gh), gAxisW[0] * gs, gAxisW[1] * gs, gAxisW[2] * gs];
        const gw = scale(gAxisW, G.w || 0);
        G.ptrs.forEach((p, i) => {
          const com = add(G.centre, qrot(gQ, sub(G.snap[i].com, gc0)));
          const v = add(G.v || [0, 0, 0], cross(gw, sub(com, G.centre)));
          writeState(p, com, qmul(gQ, G.snap[i].q), v, gw);
        });
        dumpX('place', G.ptrs);
      }
    }
    const n = P.k - P.hold;
    if (n >= 0) push(['T', ++seq, n]);
    if (n >= 0 && n === P.fire && P.piston) {
      try { fire(ptr(P.piston)); push(['F', ++seq, n]); } catch (e) { push(['E', ++seq, 'fire ' + e]); }
    }
    P.k++;
    if (n >= P.steps) { dumpX('done', P.ptrs); plan = null; flush(); }
  } catch (err) { push(['E', ++seq, String(err)]); plan = null; }
} });

Interceptor.attach(ptr('0x53ed80'), { onEnter() {
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
    const list2 = watch ? watch : Array.from(known.values());
    for (const p of list2) {
      const xf = p.add(0xcc), rot = [];
      for (let i = 0; i < 9; i++) rot.push(f(xf.add(4 * i)));
      push(['B', seq, p.toString(), f(p.add(0x190)), vec(p.add(0x30)), vec(p.add(0x54)), vec(p.add(0x64)), rot]);
    }
  } catch (err) { push(['E', ++seq, String(err)]); }
},
// V  [V,seq,body,vel(3),angvel(3)]  every watched or known body as the resolver leaves it (docs/29 12be:
//    the post-resolver velocities of a landing, for a replay of the resolver on the original's own input)
onLeave() {
  try {
    const list2 = watch ? watch : Array.from(known.values());
    for (const p of list2) push(['V', seq, p.toString(), vec(p.add(0x54)), vec(p.add(0x64))]);
  } catch (err) { push(['E', seq, String(err)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', ++seq, f(this.context.esp.add(4)), ptr('0x719724').readU32()]); } });
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
    if (plan && watch && body.equals(watch[0])) {
      const b = t.add(0x24).readPointer(), e = t.add(0x28).readPointer();
      const ws = [];
      for (let r = b; r.compare(e) < 0; r = r.add(0x50)) ws.push([r.add(4).readS32(), r.add(8).readU8()]);
      push(['G', ++seq, body.toString(), ws]);
    }
    // W  [W,seq,chassis,[[id,thrust,touching,normal(3),point(3),axle(3)] ...]]  every MagicAction apply of a chassis while a
    //    plan runs and `wheels` was asked for (docs/29 12bg: the wheel records of a fight from a placed state, as ra2_robotpair.js)
    if (plan && wheelRows) {
      const b = t.add(0x24).readPointer(), e = t.add(0x28).readPointer();
      const ws = [];
      for (let r = b; r.compare(e) < 0; r = r.add(0x50))
        ws.push([r.readS32(), r.add(4).readS32(), r.add(8).readU8(), vec(r.add(0xc)).map(R5), vec(r.add(0x2c)).map(R5), vec(r.add(0x3c)).map(R5)]);
      push(['W', seq, body.toString(), ws]);
    }
  } catch (err) { push(['E', ++seq, String(err)]); }
} });

rpc.exports = {
  flush() { flush(); return seq; },
  // Every known body: [ptr, mass, com].
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
  learnAt(addr) { learn(ptr(addr)); return f(ptr(addr).add(0x190)); },
  peek(addr, len) { return floats(ptr(addr), len); },
  peekPtrs(addr, n) { const o = []; for (let i = 0; i < n; i++) o.push(ptr(addr).add(4 * i).readPointer().toString()); return o; },
  wheels(on) { wheelRows = !!on; return true; },
  watch(list) { watch = list ? list.map(s => ptr(s)) : null; return true; },
  arm(p) {
    p.ptrs = p.bodies.map(s => ptr(s));
    for (const G of (p.extra || [])) {
      G.ptrs = G.bodies.map(s => ptr(s));
      const key = G.bodies[0];
      if (!restExtra.has(key) || G.reset) restExtra.set(key, G.ptrs.map(q => readState(q)));
      G.snap = restExtra.get(key);
      if (G.dcentre) G.centre = add(G.snap[0].com, G.dcentre);
    }
    plan = p;
    return true;
  },
  busy() { return plan !== null; },
  step() { return plan && plan.k !== undefined ? plan.k - plan.hold : -1; },
  // The immobility rule's 20 s (0x6a57f0, read by the match update 0x4134d2 and
  // 0x43bdf5) raised in this process's memory only, so that a robot left
  // standing for a set of trials is not counted out and the match not ended.
  noImmobility() {
    const p = ptr('0x6a57f0');
    Memory.protect(p, 4, 'rw-');
    const was = p.readFloat();
    p.writeFloat(1e9);
    return was;
  },
};
setInterval(flush, 300);
