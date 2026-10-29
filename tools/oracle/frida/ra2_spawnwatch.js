// Frida agent: who moves the robots' bodies outside the integrator, in the
// ORIGINAL (docs/29 section 12v): the match load puts each robot somewhere,
// then at its start point, then lowers it to the floor, and holds it until
// the bell (`spawn-*__live__000`). Every rigid body's sync (0x520750, thiscall
// on the body: rotation matrix and world centre from +0x40/+0x30, called after
// any write of the pose) is watched; a call that finds the body's centre more
// than 2 cm from where the last call left it is a teleport.
//   S  [S,seq,body,mass,old(3),new(3),q(4),v(3),[return addresses]]  (bodies of
//      20 kg and more moving slower than 20 m/s; the call stack the first time
//      a body is moved from a given caller only: the walk is costly)
//   K  [K,seq,{retaddr: calls}]  every 5 s: how often each caller syncs
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 64) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const last = new Map();
const traced = new Set();
let counts = {};
Interceptor.attach(ptr('0x520750'), { onEnter() {
  const b = this.context.ecx;
  const k = b.toString();
  const ra = this.returnAddress.toString();
  counts[ra] = (counts[ra] || 0) + 1;
  const c = vec(b.add(0x30));
  const o = last.get(k);
  last.set(k, c);
  if (o === undefined || Math.hypot(c[0] - o[0], c[1] - o[1], c[2] - o[2]) > 0.02) {
    const m = f(b.add(0x190)), v = vec(b.add(0x54));
    if (m < 20 || Math.hypot(v[0], v[1], v[2]) > 20) return;
    let bt = [];
    const key = k + ' ' + ra;
    if (!traced.has(key)) {
      traced.add(key);
      try { bt = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 10).map(a => a.toString()); } catch (e) {}
    }
    push(['S', ++seq, k, f(b.add(0x190)), o === undefined ? null : o, c,
          [f(b.add(0x40)), f(b.add(0x44)), f(b.add(0x48)), f(b.add(0x4c))], vec(b.add(0x54)), [ra].concat(bt)]);
  }
} });
setInterval(() => { push(['K', ++seq, counts]); counts = {}; flush(); }, 5000);
setInterval(flush, 300);
rpc.exports = { flush() { flush(); return seq; } };
