// Frida agent: the arenas' hinge hazards in the ORIGINAL (docs/29 section 12z):
// the hinge controllers a script's GetHinge built (vtable 0x6a7e44, the ones
// with a fixed arena body), their Havok hinge atoms, the priced hits.
//   N  [N,n]                                   every 60th stepTo (0x52aa60); n counts stepTo
//   CB [CB,n,ctl,[[where,off,ptr,mass],...]]   once per controller: the rigid bodies found among
//      the pointers of the controller and of its constraint (+0x58), one level deep
//   C  [C,n,ctl,p,lim70,lim74,S,T,maxT,motorS,autoIdle,autoStop,axisA50(3),axisB60(3),c70(3),c80(3),
//       flag91,vel_a4,angle,[[ptr,com(3),quat(4),v(3),w(3)],...]]
//      0x481450 entry, every substep: the controller's percentage +0x7c, its limits +0x70/+0x74,
//      power +0xb0..+0xbc, auto-locks +0x64/+0x65, the constraint's four vectors, its motor flag
//      +0x91 and velocity +0xa4, the controller's own angle (0x481190, called here), and its
//      dynamic bodies' states
//   K  [K,n,atom,bA,bB,mA,mB,motor81,f94,[floats +0x40..+0xbc]]  0x535a50 entry for an atom
//      with a fixed body (every 2nd call), with the bodies' w
//   KW [KW,n,atom,wA(3),wB(3)]                 same call, the bodies' angular velocities
//   P  [P,n,ctl,S,T,caller]                    0x4810e0 SetPowerSettings
//   L  [L,n,ctl,on,caller]                     0x480e20 Lock
//   M  [M,n,ctl,vel,torque,caller]             0x4815c0 the motor set
//   Q/H/A as frida/ra2_arenahits.js (computeDamage 0x489470, the gate 0x417330, the piece 0x52a990)
'use strict';
let n = 0, seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 512) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const r6 = x => Math.round(x * 1e6) / 1e6;
const vec = p => [r6(f(p)), r6(f(p.add(4))), r6(f(p.add(8)))];
const isCtl = c => { try { return c.readPointer().equals(ptr('0x6a7e44')); } catch (e) { return false; } };
const angleOf = new NativeFunction(ptr('0x481190'), 'float', ['pointer'], 'thiscall');
// A rigid body: vtable 0x6abe04 (frida/ra2_twobody.js's scan).
const isBody = p => { try { return p.readU32() === 0x6abe04; } catch (e) { return false; } };
Interceptor.attach(ptr('0x52aa60'), { onEnter() { n++; if (n % 60 === 0) push(['N', n]); } });

const ctlBodies = new Map();
function bodiesOf(c) {
  const k = c.toString();
  if (ctlBodies.has(k)) return ctlBodies.get(k);
  const found = [], seen = new Set();
  let cons = null;
  try { cons = c.add(0x58).readPointer(); } catch (e) {}
  for (const [base, nm] of [[c, 'c'], [cons, 'k']]) {
    if (base === null) continue;
    for (let i = 0; i < 0xc0; i += 4) {
      let p; try { p = base.add(i).readPointer(); } catch (e) { continue; }
      if (p.compare(ptr('0x10000')) < 0) continue;
      const cand = [[p, nm]];
      try { cand.push([p.add(0x1c).readPointer(), nm + '+1c']); } catch (e) {}
      try { cand.push([p.add(8).readPointer(), nm + '+8']); } catch (e) {}
      for (const [q, w] of cand) {
        if (isBody(q) && !seen.has(q.toString())) {
          seen.add(q.toString());
          found.push([w, i, q, f(q.add(0x190))]);
        }
      }
    }
  }
  ctlBodies.set(k, found);
  push(['CB', n, k, found.map(x => [x[0], x[1], x[2].toString(), r6(x[3])])]);
  return found;
}
const state = b => [b.toString(), vec(b.add(0x30)), [r6(f(b.add(0x40))), r6(f(b.add(0x44))), r6(f(b.add(0x48))), r6(f(b.add(0x4c)))],
  vec(b.add(0x54)), vec(b.add(0x64))];
Interceptor.attach(ptr('0x481450'), { onEnter() {
  const c = this.context.ecx;
  if (!isCtl(c)) return;
  try {
    const bs = bodiesOf(c);
    if (bs.filter(x => x[3] > 0).length >= 2) return;  // a robot's hinge: two dynamic bodies
    const k = c.add(0x58).readPointer();
    let ang = null;
    try { ang = r6(angleOf(c)); } catch (e) { ang = String(e); }
    push(['C', n, c.toString(), c.add(0x7c).readS32(), r6(f(c.add(0x70))), r6(f(c.add(0x74))),
          r6(f(c.add(0xb0))), r6(f(c.add(0xb4))), r6(f(c.add(0xb8))), r6(f(c.add(0xbc))),
          c.add(0x64).readU8(), c.add(0x65).readU8(), vec(k.add(0x50)), vec(k.add(0x60)), vec(k.add(0x70)),
          vec(k.add(0x80)), k.add(0x91).readU8(), r6(f(k.add(0xa4))), ang,
          bs.filter(x => x[3] > 0).map(x => state(x[2]))]);
  } catch (e) { push(['E', n, 'C ' + String(e)]); }
} });
const atomSeen = new Set();
let kcalls = 0;
Interceptor.attach(ptr('0x535a50'), { onEnter() {
  try {
    const a = this.context.ecx;
    const bA = a.add(0xc).readPointer(), bB = a.add(0x10).readPointer();
    const mA = f(bA.add(0x190)), mB = f(bB.add(0x190));
    if (mA !== 0 && mB !== 0) return;
    const k = a.toString();
    if (!atomSeen.has(k)) {
      atomSeen.add(k);
      const fl = []; for (let o = 0x0; o < 0xc0; o += 4) fl.push(r6(f(a.add(o))));
      push(['KP', n, k, bA.toString(), bB.toString(), r6(mA), r6(mB), fl]);
    }
    if ((kcalls++ % 2) !== 0) return;
    const fl = []; for (let o = 0x40; o < 0xc0; o += 4) fl.push(r6(f(a.add(o))));
    push(['K', n, k, a.add(0x81).readU8(), r6(f(a.add(0x94))), fl, vec(bA.add(0x64)), vec(bB.add(0x64))]);
  } catch (e) { push(['E', n, 'K ' + String(e)]); }
} });
Interceptor.attach(ptr('0x4810e0'), { onEnter() {
  const c = this.context.ecx, sp = this.context.esp;
  if (!isCtl(c)) return;
  push(['P', n, c.toString(), f(sp.add(4)), f(sp.add(8)), this.returnAddress.toString()]);
} });
Interceptor.attach(ptr('0x480e20'), { onEnter() {
  const c = this.context.ecx;
  if (!isCtl(c)) return;
  push(['L', n, c.toString(), this.context.esp.add(4).readU8(), this.returnAddress.toString()]);
} });
Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  const c = this.context.ecx, sp = this.context.esp;
  push(['M', n, c.toString(), f(sp.add(4)), f(sp.add(8)), this.returnAddress.toString()]);
} });

// The priced hits, as frida/ra2_arenahits.js.
function bodyOf(raw) {
  try {
    const p = ptr(raw >>> 0);
    if (p.isNull()) return null;
    let body = null;
    try { body = p.add(0x1c).readPointer(); } catch (e) {}
    const read = b => {
      const inv = []; for (let i = 0; i < 12; i++) inv.push(f(b.add(0x10c + 4 * i)));
      return [b.toString(), f(b.add(0x190)), vec(b.add(0x30)), inv, vec(b.add(0x54)), vec(b.add(0x64))];
    };
    let a = null, c = null;
    try { a = read(p); } catch (e) {}
    try { if (body && !body.isNull()) c = read(body); } catch (e) {}
    return [p.toString(), a, c];
  } catch (e) { return String(e); }
}
Interceptor.attach(ptr('0x489470'), { onEnter() {
  try {
    const sp = this.context.esp, rec = sp.add(8).readPointer(), c = this.context.ecx;
    const ru = [], rf = [];
    for (let i = 0; i < 16; i++) { ru.push(rec.add(4 * i).readU32()); rf.push(f(rec.add(4 * i))); }
    let mat = null;
    try {
      const m = c.add(0x64).readPointer();
      mat = [m.toString(), m.readPointer().toString(), f(m.add(4))];
    } catch (e) {}
    push(['Q', n, c.toString(), c.readPointer().toString(), [f(c.add(0x68)), f(c.add(0x6c))], mat,
          ru, rf, [bodyOf(ru[9]), bodyOf(ru[10])]]);
  } catch (e) { push(['E', n, String(e)]); }
} });
Interceptor.attach(ptr('0x417330'), { onEnter() {
  try {
    const sp = this.context.esp;
    let loc = null, nrm = null;
    try { loc = vec(sp.add(24).readPointer()); } catch (e) {}
    try { nrm = vec(sp.add(28).readPointer()); } catch (e) {}
    push(['H', n, sp.add(4).readS32(), sp.add(8).readPointer().toString(), sp.add(12).readS32(),
          sp.add(16).readPointer().toString(), f(sp.add(20)), loc, nrm]);
  } catch (e) { push(['E', n, String(e)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() { push(['A', n, f(this.context.esp.add(4))]); } });
setInterval(flush, 300);
rpc.exports = { flush() { flush(); return n; } };
