// Frida agent: weapon motors of the ORIGINAL, per Havok substep (docs/29 section 12).
//   S rows: a substep began (World::stepTo 0x52aa60): [S, n]
//   P rows: hinge SetPowerSettings 0x4810e0: [P, n, ctl, speed, torque, caller]
//   Q rows: prismatic SetPowerSettings 0x4b2af0: [Q, n, ctl, speed, force, caller]
//   M rows: hinge motor set 0x4815c0 on change: [M, n, motor, velocity, max torque]
//   H rows: hinge atom setup 0x535a50 while its motor is on (every substep):
//           [H, n, atom, target velocity, wA, wB, axisA(world-free: +0x50 of the atom)]
//   W rows: every 4th substep, each hinge seen with its motor on, motor on or off:
//           [W, n, atom, motor on +0x81, wA, wB]
//   F rows: prismatic force 0x4b51b0: [F, n, ctl, force, offset]
//   O rows: every prismatic controller each 2nd substep (its slot-5 update 0x4b4e00, called
//           locked or not): [O, n, ctl, locked +0x7c, auto-lock +0x7d, direction +0x80,
//           offset (0x4b4e10), stop +0x6c, stop +0x70, speed +0x74, force +0x78]
// n is the substep count since the agent loaded (1/60 s each).
'use strict';
let n = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
function err(e) { push(['E', n, String(e)]); }
const seenAtoms = {};
Interceptor.attach(ptr('0x52aa60'), { onEnter() {
  n++;
  if (n % 4) return;
  for (const k in seenAtoms) {
    try {
      const [a, bA, bB] = seenAtoms[k];
      push(['W', n, k, a.add(0x81).readU8(), vec(bA.add(0x64)), vec(bB.add(0x64))]);
    } catch (e) { err(e); }
  }
} });
Interceptor.attach(ptr('0x4810e0'), { onEnter() {
  try { const sp = this.context.esp; push(['P', n, this.context.ecx.toString(), f(sp.add(4)), f(sp.add(8)), this.returnAddress.toString()]); }
  catch (e) { err(e); }
} });
Interceptor.attach(ptr('0x4b2af0'), { onEnter() {
  try { const sp = this.context.esp; push(['Q', n, this.context.ecx.toString(), f(sp.add(4)), f(sp.add(8)), this.returnAddress.toString()]); }
  catch (e) { err(e); }
} });
const last = {};
Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  try {
    const m = this.context.ecx.toString(), sp = this.context.esp, v = f(sp.add(4)), t = f(sp.add(8));
    if (last[m] && last[m][0] === v && last[m][1] === t) return;
    last[m] = [v, t];
    push(['M', n, m, v, t]);
  } catch (e) { err(e); }
} });
Interceptor.attach(ptr('0x535a50'), { onEnter() {
  try {
    const a = this.context.ecx;
    if (!a.add(0x81).readU8()) return;
    const bA = a.add(0xc).readPointer(), bB = a.add(0x10).readPointer();
    seenAtoms[a.toString()] = [a, bA, bB];
    push(['H', n, a.toString(), f(a.add(0x94)), vec(bA.add(0x64)), vec(bB.add(0x64)), f(bB.add(0x190))]);
  } catch (e) { err(e); }
} });
const offsetOf = new NativeFunction(ptr('0x4b4e10'), 'float', ['pointer'], 'thiscall');
Interceptor.attach(ptr('0x4b51b0'), { onEnter() {
  try {
    const c = this.context.ecx, sp = this.context.esp;
    push(['F', n, c.toString(), f(sp.add(4)), offsetOf(c)]);
  } catch (e) { err(e); }
} });
Interceptor.attach(ptr('0x4b4e00'), { onEnter() {
  try {
    if (n % 2) return;
    const c = this.context.ecx;
    push(['O', n, c.toString(), c.add(0x7c).readU8(), c.add(0x7d).readU8(), c.add(0x80).readS32(), offsetOf(c),
          f(c.add(0x6c)), f(c.add(0x70)), f(c.add(0x74)), f(c.add(0x78))]);
  } catch (e) { err(e); }
} });
rpc.exports = { flush() { flush(); return n; } };
setInterval(flush, 300);
