// Frida agent: a raw float dump (offsets 0..0x200) of both bodies of every hinge atom the
// first times its motor runs (0x535a50, motor flag a+0x81), plus the atom itself (0..0x100),
// to read the bodies' mass, centre of mass and inverse inertia (docs/29 section 12f).
//   D rows: [D, n, atom, which ('A'|'B'|'atom'), [floats]]
'use strict';
let n = 0, rows = [];
const seen = {};
function push(r) { rows.push(r); if (rows.length >= 64) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const floats = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
Interceptor.attach(ptr('0x52aa60'), { onEnter() { n++; } });
Interceptor.attach(ptr('0x535a50'), { onEnter() {
  try {
    const a = this.context.ecx;
    if (!a.add(0x81).readU8()) return;
    const k = a.toString();
    seen[k] = (seen[k] || 0) + 1;
    if (seen[k] > 8) return;
    push(['D', n, k, 'atom', floats(a, 0x100)]);
    push(['D', n, k, 'A', floats(a.add(0xc).readPointer(), 0x200)]);
    push(['D', n, k, 'B', floats(a.add(0x10).readPointer(), 0x200)]);
  } catch (e) { push(['E', n, String(e)]); }
} });
rpc.exports = { flush() { flush(); return n; } };
setInterval(flush, 300);
