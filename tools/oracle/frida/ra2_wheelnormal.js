// Frida agent: where a wheel's drive normal comes from in the ORIGINAL
// (docs/29 section 12ad).  Function-entry/leave hooks only.
//   A  [A,seq,h]                          0x52a990: one per accepted step piece
//   N  [N,seq,wheel,ret,arg1,[arg2 dwords 0..0x40 as floats],[same as hex],
//       wheel+0x29c before (3), contact +0x284, passthru +0x288]
//      0x508010 entry: the wheel's collision-event callback (registered on its
//      `passthru` collidable at 0x507ea3; it copies arg2+4 into Wheel+0x29c).
//      arg2: +0 a speed, +4 normal (toward the wheel), +0x14 point, +0x24 the
//      wheel's collidable, +0x28 the other's
//   V  [V,seq,event,[event dwords 0..0x40 as floats],[hex]]  0x52d4f0 leave
//      when the filter lets the point through (al == 0), the event as the
//      filter left it (bodies swapped and normal negated for b's entry)
//   G  [G,seq,chassis,com(3),rot(9),vel(3),[[id,thrust,touching,normal(3),
//       point(3),axle(3)],...]]   0x4856f0 MagicAction apply entry
//   H  [H,seq,attPlayer,attComp,vicPlayer,vicComp,amount,point(3),normal(3)]
//      0x417330 entry, the damage gate
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const dw = (p, n) => { const a = [], h = []; for (let i = 0; i < n; i += 4) { a.push(f(p.add(i))); h.push(p.add(i).readU32().toString(16)); } return [a, h]; };
Interceptor.attach(ptr('0x52a990'), { onEnter() { push(['A', ++seq, f(this.context.esp.add(4))]); } });
Interceptor.attach(ptr('0x508010'), { onEnter() {
  try {
    const w = this.context.ecx, sp = this.context.esp;
    const a1 = sp.add(4).readPointer(), a2 = sp.add(8).readPointer();
    const [fl, hx] = dw(a2, 0x40);
    push(['N', ++seq, w.toString(), sp.readPointer().toString(), a1.toString(), fl, hx,
          vec(w.add(0x29c)), w.add(0x284).readPointer().toString(), w.add(0x288).readPointer().toString()]);
  } catch (e) { push(['E', ++seq, 'N ' + String(e)]); }
} });
Interceptor.attach(ptr('0x52d4f0'), {
  onEnter() { this.ev = this.context.esp.add(4).readPointer(); },
  onLeave(rv) {
    try {
      if ((rv.toUInt32() & 0xff) !== 0) return;
      const [fl, hx] = dw(this.ev, 0x40);
      push(['V', ++seq, this.ev.toString(), fl, hx]);
    } catch (e) { push(['E', ++seq, 'V ' + String(e)]); }
  } });
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const t = this.context.ecx;
    const wrap = t.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    const b = t.add(0x24).readPointer(), e = t.add(0x28).readPointer();
    const ws = [];
    for (let r = b; r.compare(e) < 0; r = r.add(0x50))
      ws.push([r.readS32(), r.add(4).readS32(), r.add(8).readU8(), vec(r.add(0xc)), vec(r.add(0x2c)), vec(r.add(0x3c))]);
    const rot = []; for (let i = 0; i < 9; i++) rot.push(f(body.add(0xcc + 4 * i)));
    push(['G', ++seq, body.toString(), vec(body.add(0x30)), rot, vec(body.add(0x54)), ws]);
  } catch (err) { push(['E', ++seq, 'G ' + String(err)]); }
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
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
