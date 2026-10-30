// Frida agent: the order of a game frame in the ORIGINAL (docs/29 section 12af).
// Function-entry hooks only.  Every row is [kind, n, ...] with n a running
// count of all rows (a total order); S counts the Havok substeps.
//   A  [A,n,h]                     0x52a990 an accepted step piece
//   S  [S,n]                       0x52aa60 World::stepTo entry (one per 1/60 s substep)
//   U  [U,n,dt]                    0x412fc0 the per-frame update, enter
//   B  [B,n,brain,dt,clock]        0x4011f0 an AI's update (brain object, frame dt, +0x44)
//   Bl [Bl,n,brain,clock]          ... and leave
//   K  [K,n,kind,dev,f1,f2]        a burst device's press callback (0x448730 piston,
//                                  0x4467f0 motor): flags +0x284/+0x285 (+0x288/+0x289) before
//   D  [D,n,kind,dev,f1,f2]        its power-draw query (0x448980 piston, 0x446d70 motor), flags before
//   T  [T,n,kind,dev,dt,f1,f2]     its component tick (0x448650, 0x446670), flags before
//   F  [F,n,kind,dev]              its fire (0x448750, 0x446810)
//   Q  [Q,n,robot]                 0x4dc3f0 the per-robot power step
//   Z  [Z,n,[4 args]]              0x4c9050 a ZoneEvent handed to Python
//   P  [P,n,leaving,[coll,tag,phantom],[coll,tag,phantom]]  0x4a4780 the zone listener (pair events)
'use strict';
let n = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const at = (a, fn) => Interceptor.attach(ptr(a), fn);
at('0x52a990', { onEnter() { push(['A', ++n, f(this.context.esp.add(4))]); } });
at('0x52aa60', { onEnter() { push(['S', ++n]); } });
at('0x412fc0', { onEnter() { push(['U', ++n, f(this.context.esp.add(4))]); } });
at('0x4011f0', {
  onEnter() { const e = this.context.ecx; this.e = e;
    push(['B', ++n, e.toString(), f(this.context.esp.add(4)), f(e.add(0x44))]); },
  onLeave() { push(['Bl', ++n, this.e.toString(), f(this.e.add(0x44))]); } });
function dev2(addr, tag, kind, o1) {
  at(addr, { onEnter() { const e = this.context.ecx;
    push([tag, ++n, kind, e.toString(), e.add(o1).readU8(), e.add(o1 + 1).readU8()]); } });
}
dev2('0x448730', 'K', 'piston', 0x284);
dev2('0x4467f0', 'K', 'motor', 0x288);
dev2('0x448980', 'D', 'piston', 0x284);
dev2('0x446d70', 'D', 'motor', 0x288);
at('0x448650', { onEnter() { const e = this.context.ecx;
  push(['T', ++n, 'piston', e.toString(), f(this.context.esp.add(4)), e.add(0x284).readU8(), e.add(0x285).readU8()]); } });
at('0x446670', { onEnter() { const e = this.context.ecx;
  push(['T', ++n, 'motor', e.toString(), f(this.context.esp.add(4)), e.add(0x288).readU8(), e.add(0x289).readU8()]); } });
at('0x448750', { onEnter() { push(['F', ++n, 'piston', this.context.ecx.toString()]); } });
at('0x446810', { onEnter() { push(['F', ++n, 'motor', this.context.ecx.toString()]); } });
at('0x4dc3f0', { onEnter() { push(['Q', ++n, this.context.ecx.toString()]); } });
at('0x4c9050', { onEnter() {
  try { const sp = this.context.esp, a = [];
    for (let i = 1; i <= 4; i++) a.push(sp.add(4 * i).readS32());
    push(['Z', ++n, a]); } catch (e) { push(['E', ++n, String(e)]); } } });
const PAIR = ptr('0x6abf6c').readU32();
at('0x4a4780', { onEnter() {
  try { const ev = this.context.esp.add(4).readPointer();
    if (ev.add(8).readU32() !== PAIR) return;
    const side = c => c.isNull() ? null : [c.toString(), c.add(0x54).readU32(), c.add(0x58).readU8()];
    push(['P', ++n, ev.add(0x14).readU32(), side(ev.add(0xc).readPointer()), side(ev.add(0x10).readPointer())]);
  } catch (e) { push(['E', ++n, String(e)]); } } });
rpc.exports = { flush() { flush(); return n; } };
setInterval(flush, 300);
