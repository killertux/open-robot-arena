// Frida agent (docs/29 12ba): ra2_robotpair.js (docs/29 12aw: the robots' meeting, every step, for a whole fight) plus the world's
// action list: AV/AS lists, AX/AY every action as it runs, AD/RM addAction/removeAction with their callers, GC which constraint group
// holds a prismatic controller's constraint (rows below), and U carries the controller's constraint pointer as its last element.
//   AV [AV,seq,[[obj,vtable,fn28] ...]]   0x52a990: the world's action vector (world + 0x30 .. + 0x34), when it changes
//   AS [AS,seq,[[obj,vtable,fn14,priority] ...]]   the list of the one world action (0x530fd0, list at +0x90 .. +0x94), when it changes
//   AX [AX,seq,fn,this]  one run of the world action;  AY [AY,seq,fn14,this]  one run of each entry of its list, in list order
//   AD [AD,seq,action,fn14,flag,caller]  0x4ad100 addAction;  RM [RM,seq,action,fn14,caller]  0x4ad130 removeAction
//   GC [GC,seq,group,ctx,constraint,offset,offset2]  0x534480: a solver context that holds a controller's constraint
// Derived from ra2_robotpair.js (below).
// Derived from ra2_fightcontacts.js; function-entry hooks only (the physics code is x87-heavy).
//   A  [A,seq,h]                       0x52a990 entry: one per accepted step piece
//   S  [S,seq,[[body,com(3),vel(3),ang(3),rot(9)] x chassis]]   at each resolver entry (0x53ed80): the chassis
//      bodies learned from the MagicAction hook (0x4856f0), in the order learned
//   C  [C,seq,[[tagA,tagB,robotA,robotB,n(3),p(3),dist,muA,muB,eA,eB] ...]]   resolver entry: only the contacts
//      between collidables of two different robots (collidable +0x54 >> 4 non-zero and unequal)
//   H  [H,seq,att,attComp,vic,vicComp,amount,loc]  0x417330: a priced hit
//   U  [U,seq,ctrl,vtable,dir +0x80,stop6c,stop70,speed +0x74,force +0x78,locked +0x7c,autolock +0x7d,rodBody,com(3),vel(3)]
//      0x4b5240 prismatic controller update (every controller: the Wide Load's burst piston and the arena's)
//   F  [F,seq,ctrl,force,caller]       0x4b51b0 one push
//   L  [L,seq,ctrl,on,caller]          0x4b50c0 lock
//   BF [BF,seq,this]                   0x448750 burst piston fire
//   MK [MK,seq,player]                 rpc mark(player)
//   W  [W,seq,chassis,[[id,thrust,touching,normal(3),point(3),axle(3)] ...]]  0x4856f0: every MagicAction apply, the wheel records
//   N  [N,seq,wheel,normal(3),point(3),otherTag,otherRobot,speed]  0x508010: a wheel's collision-event callback
//   V  [V,seq,tagA,tagB,robotA,robotB,ret]  0x52d4f0 (the event filter) leave: every call between two robots' collidables;
//      ret 0 = the event goes on (the wheels' callbacks hear it)
//   R  [R,seq,[[mass,com(3),vel(3),ang(3)] ...]]  at each resolver entry: the bodies the prismatic controllers' constraints hold (chassis, rod)
//   FM [FM,seq,this]                   0x446810 burst motor fire
//   U also carries the bodies the controller's constraint holds: [[off,body,mass,com,vel]]
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const chassis = new Map();
const ctrlBodies = new Map();
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
    if (ctrlBodies.size) {
      const rb = [], seen = new Set();
      for (const found of ctrlBodies.values()) for (const x of found) {
        const k = x[1].toString(); if (seen.has(k)) continue; seen.add(k);
        rb.push([R(x[2]), ...vec(x[1].add(0x30)).map(R), ...vec(x[1].add(0x54)).map(R), ...vec(x[1].add(0x64)).map(R)]);
      }
      push(['R', seq, rb]);
    }
  } catch (err) { push(['E', seq, String(err)]); }
} });
// AV: the world's action list (world + 0x30 .. + 0x34, 4-byte object pointers; 0x52a990 calls vtable +0x28 of each in order):
//   [AV,seq,[[obj,vtable,fn28] ...]] when the list changes; AX [AX,seq,fn28,obj]: one action's apply (docs/29 12ba)
let avSig = '';
const axHooked = new Set();
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', seq, f(this.context.esp.add(4))]);
  try {
    const w = this.context.ecx;
    const b = w.add(0x30).readPointer(), e = w.add(0x34).readPointer();
    const l = [];
    for (let r = b; r.compare(e) < 0 && l.length < 200; r = r.add(4)) {
      const o = r.readPointer(), vt = o.readPointer(), fn = vt.add(0x28).readPointer();
      l.push([o.toString(), vt.toString(), fn.toString()]);
      if (!axHooked.has(fn.toString())) {
        axHooked.add(fn.toString());
        Interceptor.attach(fn, { onEnter() {
          const a = this.context.ecx;
          push(['AX', seq, fn.toString(), a.toString()]);
          try {
            const b = a.add(0x90).readPointer(), e = a.add(0x94).readPointer(), l = [];
            for (let r = b; !b.isNull() && r.compare(e) < 0 && l.length < 100; r = r.add(8)) {
              const o = r.readPointer(), vt = o.readPointer(), f14 = vt.add(0x14).readPointer();
              l.push([o.toString(), vt.toString(), f14.toString(), r.add(4).readS32()]);
              if (!axHooked.has('s' + f14.toString())) {
                axHooked.add('s' + f14.toString());
                Interceptor.attach(f14, { onEnter() { curGroup = this.context.ecx; push(['AY', seq, f14.toString(), this.context.ecx.toString()]); } });
              }
            }
            const sg = l.map(x => x[0]).join(',');
            if (sg !== (this.sg || globalThis.__sg)) { globalThis.__sg = sg; push(['AS', seq, l]); }
          } catch (err) { push(['E', seq, 'AS ' + String(err)]); }
        } });
      }
    }
    const sig = l.map(x => x[0]).join(',');
    if (sig !== avSig) { avSig = sig; push(['AV', seq, l]); }
  } catch (err) { push(['E', seq, 'AV ' + String(err)]); }
} });
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const t = this.context.ecx;
    const wrap = t.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    if (!body.isNull() && !chassis.has(body.toString())) { chassis.set(body.toString(), body); push(['G', seq, body.toString(), f(body.add(0x190))]); }
    const b = t.add(0x24).readPointer(), e = t.add(0x28).readPointer();
    const ws = [];
    for (let r = b; r.compare(e) < 0; r = r.add(0x50))
      ws.push([r.readS32(), r.add(4).readS32(), r.add(8).readU8(), vec(r.add(0xc)).map(R), vec(r.add(0x2c)).map(R), vec(r.add(0x3c)).map(R)]);
    push(['W', seq, body.toString(), ws]);
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
const fa = a => new Float32Array(new Int32Array([a.toInt32()]).buffer)[0];
Interceptor.attach(ptr('0x4b5240'), { onEnter() {
  try {
    const c = this.context.ecx;
    let rod = null, com = null, vel = null;
    const key = c.toString();
    if (!ctrlBodies.has(key)) {
      const found = [];
      const cons = c.add(0x58).readPointer();
      const sane = b => { try { const m = f(b.add(0x190)); return m > 0.5 && m < 1e5 ? m : 0; } catch (e) { return 0; } };
      for (const [base, nm] of [[c, 'c'], [cons, 'k']]) for (let i = 0; i < 0x80; i += 4) {
        let p; try { p = base.add(i).readPointer(); } catch (e) { continue; }
        if (p.compare(ptr('0x10000')) < 0) continue;
        const m = sane(p); if (m) found.push([nm + i, p, m]);
        try { const q = p.add(0x1c).readPointer(); const m2 = sane(q); if (m2) found.push([nm + i + '+1c', q, m2]); } catch (e) {}
      }
      ctrlBodies.set(key, found);
    }
    com = ctrlBodies.get(key).map(x => [x[0], x[1].toString(), R(x[2]), vec(x[1].add(0x30)).map(R), vec(x[1].add(0x54)).map(R)]);
    push(['U', seq, c.toString(), c.readPointer().toString(), c.add(0x80).readS32(), R(f(c.add(0x6c))), R(f(c.add(0x70))),
          R(f(c.add(0x74))), R(f(c.add(0x78))), c.add(0x7c).readU8(), c.add(0x7d).readU8(), rod, com, vel, c.add(0x58).readPointer().toString()]);
    ctrlCons.add(c.add(0x58).readPointer().toString());
  } catch (e) { push(['E', seq, String(e)]); }
} });
Interceptor.attach(ptr('0x4b51b0'), { onEnter(a) { push(['F', seq, this.context.ecx.toString(), R(fa(a[0])), this.returnAddress.toString()]); } });
Interceptor.attach(ptr('0x4b50c0'), { onEnter(a) { push(['L', seq, this.context.ecx.toString(), a[0].toInt32() & 0xff, this.returnAddress.toString()]); } });
Interceptor.attach(ptr('0x448750'), { onEnter() { push(['BF', seq, this.context.ecx.toString()]); } });
Interceptor.attach(ptr('0x508010'), { onEnter() {
  try {
    const w = this.context.ecx, sp = this.context.esp;
    const a2 = sp.add(8).readPointer();
    const other = a2.add(0x28).readPointer();
    push(['N', seq, w.toString(), vec(a2.add(4)).map(R), vec(a2.add(0x14)).map(R), other.add(0x50).readU32(), other.add(0x54).readU32() >> 4, R(f(a2))]);
  } catch (e) { push(['E', seq, 'N ' + String(e)]); }
} });
Interceptor.attach(ptr('0x446810'), { onEnter() { push(['FM', seq, this.context.ecx.toString()]); } });
Interceptor.attach(ptr('0x52d4f0'), {
  onEnter() { this.ev = this.context.esp.add(4).readPointer(); },
  onLeave(rv) {
    try {
      const ev = this.ev;
      const a = ev.add(0x10).readPointer(), b = ev.add(0x14).readPointer();
      const ra = a.add(0x54).readU32() >> 4, rb = b.add(0x54).readU32() >> 4;
      if (ra === 0 || rb === 0 || ra === rb) return;
      push(['V', seq, a.add(0x50).readU32(), b.add(0x50).readU32(), ra, rb, rv.toUInt32() & 0xff]);
    } catch (e) { push(['E', seq, 'V ' + String(e)]); }
  } });
// AD/RM: 0x4ad100 addAction(action, flag) / 0x4ad130 removeAction(action) with the caller; GC: which 523bd0 action's solver
// context (0x534480) holds a prismatic controller's constraint (docs/29 12ba)
const ctrlCons = new Set(), gcSeen = new Set();
let curGroup = null;
function fn14(o) { try { return o.readPointer().add(0x14).readPointer().toString(); } catch (e) { return '?'; } }
Interceptor.attach(ptr('0x4ad100'), { onEnter(a) { const o = this.context.esp.add(4).readPointer(); push(['AD', seq, o.toString(), fn14(o), this.context.esp.add(8).readU8(), this.returnAddress.toString()]); } });
Interceptor.attach(ptr('0x4ad130'), { onEnter(a) { const o = this.context.esp.add(4).readPointer(); push(['RM', seq, o.toString(), fn14(o), this.returnAddress.toString()]); } });
Interceptor.attach(ptr('0x534480'), { onEnter() {
  try {
    if (!ctrlCons.size) return;
    const ctx = this.context.ecx;
    for (let i = 0; i < 0x100; i += 4) {
      let p; try { p = ctx.add(i).readPointer(); } catch (e) { continue; }
      if (ctrlCons.has(p.toString())) { const k = curGroup + p + i; if (!gcSeen.has(k)) { gcSeen.add(k); push(['GC', seq, String(curGroup), ctx.toString(), p.toString(), i, 0]); } }
      if (p.compare(ptr('0x10000')) < 0) continue;
      for (let j = 0; j < 0x100; j += 4) {
        let q; try { q = p.add(j).readPointer(); } catch (e) { break; }
        if (ctrlCons.has(q.toString())) { const k = curGroup + q + i + ':' + j; if (!gcSeen.has(k)) { gcSeen.add(k); push(['GC', seq, String(curGroup), ctx.toString(), q.toString(), i, j]); } }
      }
    }
  } catch (e) { push(['E', seq, 'GC ' + String(e)]); }
} });
let player = -1;
rpc.exports = { flush() { flush(); return seq; }, mark(p) { player = p; push(['MK', ++seq, p]); } };
setInterval(flush, 300);
