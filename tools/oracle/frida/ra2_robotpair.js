// Frida agent (docs/29 12aw): the robots' meeting, every step, for a whole fight.
// Derived from ra2_fightcontacts.js; function-entry hooks only (the physics code is x87-heavy).
//   A  [A,seq,h]                       0x52a990 entry: one per accepted step piece
//   S  [S,seq,[[body,com(3),vel(3),ang(3),rot(9)] x chassis]]   at each resolver entry (0x53ed80): the chassis
//      bodies learned from the MagicAction hook (0x4856f0), in the order learned
//   C  [C,seq,[[tagA,tagB,robotA,robotB,n(3),p(3),dist,muA,muB,eA,eB] ...]]   resolver entry: only the contacts
//      between collidables of two different robots (collidable +0x54 >> 4 non-zero and unequal)
//   H  [H,seq,att,attComp,vic,vicComp,amount,loc]  0x417330: a priced hit
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const chassis = new Map();
const R = x => Math.round(x * 1e5) / 1e5;
Interceptor.attach(ptr('0x53ed80'), { onEnter() {
  try {
    const sp = this.context.esp;
    const list = sp.add(4).readPointer();
    const b = list.add(4).readPointer(), e = list.add(8).readPointer();
    seq++;
    if (!b.isNull()) {
      const out = [];
      let n = 0;
      for (let r = b; r.compare(e) < 0 && n < 400; r = r.add(0x74), n++) {
        const ca = r.add(8).readPointer(), cb = r.add(0xc).readPointer();
        const ra = ca.add(0x54).readU32() >> 4, rb = cb.add(0x54).readU32() >> 4;
        if (ra === 0 || rb === 0 || ra === rb) continue;
        out.push([ca.add(0x50).readU32(), cb.add(0x50).readU32(), ra, rb, ...vec(r.add(0x10)).map(R), ...vec(r.add(0x20)).map(R),
                  R(f(r.add(0x54))), f(r.add(0x58)), f(r.add(0x5c)), f(r.add(0x60)), f(r.add(0x64))]);
      }
      if (out.length) push(['C', seq, out]);
    }
    const s = [];
    for (const [k, p] of chassis) {
      const xf = p.add(0xcc), rot = [];
      for (let i = 0; i < 9; i++) rot.push(R(f(xf.add(4 * i))));
      s.push([k, ...vec(p.add(0x30)).map(R), ...vec(p.add(0x54)).map(R), ...vec(p.add(0x64)).map(R), ...rot]);
    }
    if (s.length) push(['S', seq, s]);
  } catch (err) { push(['E', seq, String(err)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() { push(['A', seq, f(this.context.esp.add(4))]); } });
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const t = this.context.ecx;
    const wrap = t.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    if (!body.isNull() && !chassis.has(body.toString())) { chassis.set(body.toString(), body); push(['G', seq, body.toString(), f(body.add(0x190))]); }
  } catch (err) { push(['E', seq, String(err)]); }
} });
Interceptor.attach(ptr('0x417330'), { onEnter() {
  try {
    const sp = this.context.esp;
    let loc = null;
    try { const p = sp.add(24).readPointer(); loc = [f(p), f(p.add(4)), f(p.add(8))]; } catch (e) {}
    push(['H', seq, sp.add(4).readS32(), sp.add(8).readPointer().toString(), sp.add(12).readS32(),
          sp.add(16).readPointer().toString(), f(sp.add(20)), loc]);
  } catch (e) { push(['E', seq, String(e)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
