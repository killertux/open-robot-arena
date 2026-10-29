// Frida agent: the priced hits of a fight in the ORIGINAL with everything the
// pricing reads, to recover how a contact with the arena is priced (docs/29
// section 12o).  Function-entry hooks only.
//   Q  [Q,seq,comp,compVtable,[piercing +0x68, concussion +0x6c],
//       [materialPtr +0x64, materialVtable, materialScale +4],
//       record u32[16], record f32[16], bodies]
//      0x489470 computeDamage entry, thiscall(component, ?, record*): the
//      attacking component, its own hit parameters and material, and the hit
//      record (0x45a430: +0 rec0, +4 normal, +0x14 point, +0x24/+0x28 the two
//      sides, +0x2c/+0x30 their masses, +0x38/+0x3c ids).  bodies = for the
//      record's +0x24 and +0x28: [ptr, collidable+0x1c body, mass +0x190,
//      com +0x30 (3), world inverse inertia +0x10c (12), linear velocity +0x54 (3),
//      angular velocity +0x64 (3)] read from the body when the pointer is one
//   H  [H,seq,attPlayer,attComp,vicPlayer,vicComp,amount,point(3),normal(3)]
//      0x417330 entry: the arena's gate (amount >= 10 is applied)
//   A  [A,seq,h]  0x52a990: one per accepted Havok step piece (sim time)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
function bodyOf(raw) {
  // The record's side is a collidable (+0x1c its rigid body) or a rigid body.
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
    push(['Q', ++seq, c.toString(), c.readPointer().toString(), [f(c.add(0x68)), f(c.add(0x6c))], mat,
          ru, rf, [bodyOf(ru[9]), bodyOf(ru[10])]]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x417330'), { onEnter() {
  try {
    const sp = this.context.esp;
    let loc = null, nrm = null;
    try { loc = vec(sp.add(24).readPointer()); } catch (e) {}
    try { nrm = vec(sp.add(28).readPointer()); } catch (e) {}
    push(['H', ++seq, sp.add(4).readS32(), sp.add(8).readPointer().toString(), sp.add(12).readS32(),
          sp.add(16).readPointer().toString(), f(sp.add(20)), loc, nrm]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() { push(['A', ++seq, f(this.context.esp.add(4))]); } });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
