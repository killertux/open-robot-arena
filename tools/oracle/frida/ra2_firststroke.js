// Frida agent: ra2_parts.js plus, every step piece, the pose of EVERY Havok body the
// integrator (0x520fb0) touches (docs/29 section 12as):
//   BP [BP,seq,idx,ptr,floats 0..0x200]  once per body (mass +0x190, inertia, ...)
//   B  [B,seq,[[idx,com(3),rot(9),vel(3),angvel(3)],...]]  every piece
//   D  [D,seq,which,ptr,floats]  the hinge atoms (0x535a50, motor on) and prismatic controllers (0x4b51b0), 3 times each
// Component damage and break-off in a fight of the ORIGINAL, with
// component identity (docs/29 section 12ar).  Function-entry/leave hooks only.
//   K  [K,seq,player,id,comp,hp +0xec,maxhp +0xe8,fracture +0xdc,state +0xe4,detach +0xe0,robot]
//        0x4dd760 (robot.componentById) leave: the id -> component pointer map,
//        filled by the driver calling GetComponentHealth(i) for every id of each
//        player right after the start (the MK row says whose ids follow)
//   MK [MK,seq,player]                 rpc mark(player)
//   C  [C,seq,comp,blow,hp,fracture,state,detach,+0xf0,caller,(backtrace of the first 40 blows under 10)]   0x45f2b0 entry: one blow applied
//        to an ordinary component (the original's per-class apply)
//   R  [R,seq,comp,hp,detach]          0x45f2b0 leave: hitpoints and detach state after
//   Q  [Q,seq,comp,arg]                0x417d70 entry: the detach is queued
//   F  [F,seq,comp,flag,detach]        0x45f4e0 entry: the detach is done (flag = param_1)
//   T  [T,seq,comp,detach]             0x45f710 entry: the detach state machine, one per
//                                      component taken with it (the children recurse)
//   X  [X,seq,comp,blow,hp]            0x44f370 entry: a blow on a chassis
//   O  [O,seq,this,a1,reason]          0x418040 entry: a robot is out (reason 1 destroyed,
//                                      2, 3), the match's own rule
//   W  [W,seq,this,winner,second]      0x4170e0 entry: the match ends (FUN_004170e0(winner, partner))
//   H  [H,seq,attPlayer,attComp,vicPlayer,vicComp,amount,point(3),normal(3)]  0x417330
//   A  [A,seq,h]                       0x52a990: one per accepted step piece
//   MC [MC,seq,elapsed,remaining,fighting,over]  the match clock (0x7162c8 +0xe4/+0xe0/+0x8e/+0x8f), every 15th piece
//   M  [M,seq,motor,velocity,max torque]  0x4815c0 hinge motor set, on change
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
let player = -1, bt = 0;
const seenK = new Set();
const stat = c => { try { return [f(c.add(0xec)), f(c.add(0xdc)), c.add(0xe4).readS32(), c.add(0xe0).readS32()]; } catch (e) { return null; } };
Interceptor.attach(ptr('0x4dd760'), {
  onEnter() { this.id = this.context.esp.add(4).readS32(); this.robot = this.context.ecx.toString(); },
  onLeave(ret) {
    try {
      const c = ret;
      if (c.isNull()) return;
      const key = this.robot + '/' + this.id + '/' + c + '/' + player;
      if (seenK.has(key)) return;
      seenK.add(key);
      push(['K', ++seq, player, this.id, c.toString(), f(c.add(0xec)), c.add(0xe8).readS32(),
            f(c.add(0xdc)), c.add(0xe4).readS32(), c.add(0xe0).readS32(), this.robot]);
    } catch (e) { push(['E', ++seq, 'K ' + String(e)]); }
  } });
Interceptor.attach(ptr('0x45f2b0'), {
  onEnter() {
    try {
      const c = this.context.ecx; this.c = c;
      const s = stat(c) || [];
      const blow = f(this.context.esp.add(4));
      const row = ['C', ++seq, c.toString(), blow, s[0], s[1], s[2], s[3], c.add(0xf0).readU32(),
                   this.returnAddress.toString()];
      // The first few small blows (under the 10 hp gate): who calls the apply.
      if (blow < 10 && bt < 40) { bt++; row.push(Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 8).map(String)); }
      push(row);
    } catch (e) { push(['E', ++seq, 'C ' + String(e)]); }
  },
  onLeave() {
    try { const s = stat(this.c) || []; push(['R', ++seq, this.c.toString(), s[0], s[3]]); } catch (e) {}
  } });
Interceptor.attach(ptr('0x417d70'), { onEnter() {
  try { push(['Q', ++seq, this.context.ecx.toString(), this.context.esp.add(4).readU32()]); } catch (e) {}
} });
Interceptor.attach(ptr('0x45f4e0'), { onEnter() {
  try { const c = this.context.ecx; push(['F', ++seq, c.toString(), this.context.esp.add(4).readU8(), c.add(0xe0).readS32()]); } catch (e) {}
} });
Interceptor.attach(ptr('0x45f710'), { onEnter() {
  try { const c = this.context.ecx; push(['T', ++seq, c.toString(), c.add(0xe0).readS32()]); } catch (e) {}
} });
Interceptor.attach(ptr('0x44f370'), { onEnter() {
  try { const c = this.context.ecx; push(['X', ++seq, c.toString(), f(this.context.esp.add(4)), f(c.add(0xec))]); } catch (e) {}
} });
Interceptor.attach(ptr('0x418040'), { onEnter() {
  try { const sp = this.context.esp; push(['O', ++seq, this.context.ecx.toString(), sp.add(4).readU32(), sp.add(8).readS32()]); } catch (e) {}
} });
Interceptor.attach(ptr('0x4170e0'), { onEnter() {
  try { const sp = this.context.esp; push(['W', ++seq, this.context.ecx.toString(), sp.add(4).readS32(), sp.add(8).readS32()]); } catch (e) {}
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
let nA = 0;
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', ++seq, f(this.context.esp.add(4))]);
  // The match clock (docs/24): +0xe4 elapsed since the bell, +0xe0 remaining, +0x8e fighting, +0x8f over.
  if (++nA % 15 === 0) {
    try { const m = ptr('0x7162c8'); push(['MC', seq, f(m.add(0xe4)), f(m.add(0xe0)), m.add(0x8e).readU8(), m.add(0x8f).readU8()]); } catch (e) {}
  }
} });
const lastMotor = {};
Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  try {
    const m = this.context.ecx.toString(), sp = this.context.esp, v = f(sp.add(4)), t = f(sp.add(8));
    if (lastMotor[m] && lastMotor[m][0] === v && lastMotor[m][1] === t) return;
    lastMotor[m] = [v, t];
    push(['M', ++seq, m, v, t]);
  } catch (e) {}
} });

// The splash (docs/29 section 12ar): a blow on a chassis (0x44f370) is passed, scaled by
// (1 - hp/maxhp), to the components its robot lists (FUN_004df200 -> FUN_004e2500 -> one
// FUN_004e2540 per listed handle): each gets min(1 - distance, b) times that amount.
//   S  [S,seq,comp,state 0x45ed20,amount ev+0,point ev+0x40(3),pos vtable[1](3),x=1-d,b,event floats +4..+0x3c]
const getComp = new NativeFunction(ptr('0x4a5c40'), 'pointer', ['pointer'], { abi: 'thiscall' });
const compState = new NativeFunction(ptr('0x45ed20'), 'int', ['pointer'], { abi: 'thiscall' });
let cur = null;
Interceptor.attach(ptr('0x4e2540'), {
  onEnter() {
    try {
      const ev = this.context.ecx, h = ptr(this.context.esp.add(4).readU32());
      const c = getComp(h);   // thiscall: the first argument is ecx
      cur = { ev: ev, comp: c };
    } catch (e) { cur = null; push(['E', ++seq, 'S ' + String(e)]); }
  } });
Interceptor.attach(ptr('0x420370'), {
  onEnter() {
    try {
      if (this.returnAddress.toString() !== '0x4e25d1' || !cur) return;
      const sp = this.context.esp, a = sp.add(4).readPointer(), b = sp.add(8).readPointer();
      const ev = cur.ev, c = cur.comp;
      let state = null, pos = null;
      try { state = compState(c); } catch (e) {}
      try {
        const vt = c.readPointer(), out = Memory.alloc(64);
        const fn = new NativeFunction(vt.add(4).readPointer(), 'pointer', ['pointer', 'pointer'], { abi: 'thiscall' });
        const r = fn(c, out);
        const pp = r.isNull() ? out : r;
        pos = vec(pp);
      } catch (e) { pos = String(e); }
      const evf = []; for (let i = 0; i < 0x13; i++) evf.push(f(ev.add(4 * i)));
      push(['S', ++seq, c.toString(), state, f(ev), vec(ev.add(0x40)), pos, f(a), f(b), evf]);
    } catch (e) { push(['E', ++seq, 'S2 ' + String(e)]); }
  } });
rpc.exports = { flush() { flush(); return seq; }, mark(p) { player = p; push(['MK', ++seq, p]); } };
setInterval(flush, 300);

const bodies = new Map(), order = [];
const fl = (p, len) => { const o = []; for (let i = 0; i < len; i += 4) o.push(p.add(i).readFloat()); return o; };
const r4 = x => Math.round(x * 1e5) / 1e5;
Interceptor.attach(ptr('0x520fb0'), { onEnter() {
  try {
    const b = this.context.ecx, k = b.toString();
    if (bodies.has(k)) return;
    if (!(f(b.add(0x190)) > 0)) return;
    bodies.set(k, order.length); order.push(b);
    push(['BP', seq, order.length - 1, k, fl(b, 0x200)]);
  } catch (e) { push(['E', ++seq, 'BP ' + String(e)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  try {
    const m = ptr('0x7162c8');
    if (!m.add(0x8e).readU8() || f(m.add(0xe4)) > (typeof SECS === 'number' ? SECS : 12)) return;
    const o = [];
    for (let i = 0; i < order.length; i++) {
      const b = order[i], r = [i];
      for (const v of vec(b.add(0x30))) r.push(r4(v));
      for (let j = 0; j < 9; j++) r.push(r4(f(b.add(0xcc + 4 * j))));
      for (const v of vec(b.add(0x54))) r.push(r4(v));
      for (const v of vec(b.add(0x64))) r.push(r4(v));
      o.push(r);
    }
    push(['B', seq, o]);
  } catch (e) { push(['E', ++seq, 'B ' + String(e)]); }
} });
const seenD = {};
function dumpOnce(tag, a, len) {
  const k = tag + a.toString();
  seenD[k] = (seenD[k] || 0) + 1;
  if (seenD[k] > 3) return;
  push(['D', seq, tag, a.toString(), fl(a, len)]);
}
Interceptor.attach(ptr('0x535a50'), { onEnter() {
  try { const a = this.context.ecx; if (!a.add(0x81).readU8()) return; dumpOnce('hinge', a, 0x100);
        for (const w of [0xc, 0x10]) dumpOnce('hingebody' + w, a.add(w).readPointer(), 0x40); } catch (e) {}
} });
Interceptor.attach(ptr('0x4b51b0'), { onEnter() {
  try { dumpOnce('prism', this.context.ecx, 0x100); } catch (e) {}
} });

// The contacts of the first seconds and how each is priced (docs/29 section 12as).
//   RC [RC,seq,tol,[[bodyA idx|ptr,bodyB idx|ptr,normal(3),point(3),distance,collA,collB],...]]
//        0x53ed80 resolver driver entry: every contact of the piece (capped at 120)
//   QD [QD,seq,comp,[rec 0..5],arg]  0x489470 computeDamage entry: rec0 is the closing speed
function inWindow() {
  const m = ptr('0x7162c8');
  return m.add(0x8e).readU8() && f(m.add(0xe4)) <= (typeof SECS === 'number' ? SECS : 12);
}
Interceptor.attach(ptr('0x53ed80'), { onEnter() {
  try {
    if (!inWindow()) return;
    const sp = this.context.esp, list = sp.add(4).readPointer(), tol = f(sp.add(8));
    const b = list.add(4).readPointer(), e = list.add(8).readPointer();
    if (b.isNull()) return;
    const out = [];
    let n = 0;
    for (let r = b; r.compare(e) < 0 && n < 120; r = r.add(0x74), n++) {
      const ca = r.add(8).readPointer(), cb = r.add(0xc).readPointer();
      const ba = ca.add(0x1c).readPointer().toString(), bb = cb.add(0x1c).readPointer().toString();
      const ia = bodies.has(ba) ? bodies.get(ba) : ba, ib = bodies.has(bb) ? bodies.get(bb) : bb;
      out.push([ia, ib, vec(r.add(0x10)).map(r4), vec(r.add(0x20)).map(r4), r4(f(r.add(0x54))), ca.toString(), cb.toString()]);
    }
    push(['RC', seq, r4(tol), out]);
  } catch (e) { push(['E', ++seq, 'R ' + String(e)]); }
} });
Interceptor.attach(ptr('0x489470'), { onEnter() {
  try {
    if (!inWindow()) return;
    const sp = this.context.esp, rec = sp.add(8).readPointer();
    const r = []; for (let i = 0; i < 6; i++) r.push(f(rec.add(4 * i)));
    push(['QD', ++seq, this.context.ecx.toString(), r, sp.add(4).readU32()]);
  } catch (e) { push(['E', ++seq, 'Q ' + String(e)]); }
} });
