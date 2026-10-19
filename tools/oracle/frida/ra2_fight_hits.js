// Frida agent: every priced collision hit of the ORIGINAL during a fight.
// Function-entry hooks only.
//   P  [P,seq,comp,rec(6),player?]  0x489470 computeDamage entry, thiscall(component, ?, record*)
//      (the attacking component; record[0] = rec0, the normal closing speed)
//   H  [H,seq,attPlayer,attComp,vicPlayer,vicComp,amount,loc(3)]  0x417330 entry: the
//      arena's gate (amount >= 10 is applied and scored, FUN_004175b0)
//   A  [A,seq,h]  0x52a990: one per accepted Havok step piece (sim time)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
Interceptor.attach(ptr('0x489470'), { onEnter() {
  try {
    const sp = this.context.esp, rec = sp.add(8).readPointer();
    const r = []; for (let i = 0; i < 6; i++) r.push(f(rec.add(4 * i)));
    push(['P', ++seq, this.context.ecx.toString(), r, sp.add(4).readU32()]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x417330'), { onEnter() {
  try {
    const sp = this.context.esp;
    let loc = null;
    try { const p = sp.add(24).readPointer(); loc = [f(p), f(p.add(4)), f(p.add(8))]; } catch (e) {}
    push(['H', ++seq, sp.add(4).readS32(), sp.add(8).readPointer().toString(), sp.add(12).readS32(),
          sp.add(16).readPointer().toString(), f(sp.add(20)), loc]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() { push(['A', ++seq, f(this.context.esp.add(4))]); } });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
