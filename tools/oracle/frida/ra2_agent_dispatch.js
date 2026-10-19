// Frida agent: which collision agent every pair of collidables meets in the ORIGINAL
// (0x543370 entry: collidable +0x18 is the shape class, +0x1c the body, body +0x190 the mass).
// D rows: first and every 60th call per pair; A rows: step pieces (agent-dispatch-* captures).
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const seen = new Map();
Interceptor.attach(ptr('0x543370'), { onEnter() {
  try {
    const sp = this.context.esp;
    const a = sp.add(4).readPointer(), b = sp.add(8).readPointer();
    const key = a.toString() + ' ' + b.toString();
    const n = (seen.get(key) || 0) + 1; seen.set(key, n);
    if (n === 1 || n % 60 === 0) {
      const ba = a.add(0x1c).readPointer(), bb = b.add(0x1c).readPointer();
      push(['D', ++seq, a.toString(), b.toString(), a.add(0x18).readS32(), b.add(0x18).readS32(),
            f(ba.add(0x190)), f(bb.add(0x190)), n]);
    }
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() { push(['A', ++seq, f(this.context.esp.add(4))]); } });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
