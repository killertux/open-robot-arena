// Frida agent: log every call into the original's damage routine.
//
// FUN_004175b0 (thiscall) is the common sink reached from plus.damage
// (FUN_004cd320 -> FUN_004148f0, which passes attacker=-1) and from the two
// engine paths FUN_00417330 / FUN_00417ae0 (collision damage).  Observed
// argument layout from the plus.damage call site:
//   a0 attacker player (-1 = environment/script), a1 ?, a2 victim player,
//   a3 component object*, a4 amount (float), a5/a6 vec3* (location, normal?),
//   a7 flag.
// Every stack arg is logged both as u32 and as float so the semantics can be
// confirmed from data; send() messages are JSON lines.
'use strict';
const DAMAGE = ptr('0x4175b0');
let n = 0;
const t0 = Date.now();

function vec3(p) {
  try { return [p.readFloat(), p.add(4).readFloat(), p.add(8).readFloat()]; } catch (e) { return null; }
}

Interceptor.attach(DAMAGE, {
  onEnter() {
    const sp = this.context.esp;
    const a = [];
    for (let i = 0; i < 8; i++) a.push(sp.add(4 + 4 * i).readU32());
    const f = a.map(v => { const b = Memory.alloc(4); b.writeU32(v); return b.readFloat(); });
    send({ ev: 'damage', n: ++n, ms: Date.now() - t0, from: this.returnAddress.toString(),
           ecx: this.context.ecx.toString(), u32: a, f32: f,
           v5: vec3(ptr(a[5])), v6: vec3(ptr(a[6])) });
  }
});
rpc.exports = { count() { return n; } };
