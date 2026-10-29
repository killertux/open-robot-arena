// Frida agent: a Steering unit's wheel constraints in motion, and the
// detector's wakes during the build, in the ORIGINAL (docs/29 section 12y).
// Loaded before Start Game (capture_wheel.py, through ra2_hook.py).
//   Y  [Y,sys,[[body,mass],...]]               a robot system seen first (as ra2_pieces.js)
//   M  [M,n,body,mass,flags]                   a watched body's mass or +0x1c flags changed
//   BELL [BELL,n,body]                          a held chassis let go: the wheel window opens
//   A  [A,n,body,active,forget,flags,mass,caller]   setActive (0x523230) on a watched body
//   DW [DW,n,caller,[[pair,ca,cb,ba,bb,fa,fb,ma,mb,[ca xyz],[cb xyz]],...]]
//      the detector's wake pass (0x548da0): every pair of its list with a body
//      that would be woken (+0x1c bits 0..2 clear), with both collidables and
//      bodies (flags, mass, centre)
//   DE [DE,n,[body,flags,mass,com],caller,[[A,B] held-back pairs with it],count]  0x548ef0
// From the bell for WHEEL_STEPS stepTo calls:
//   WS [WS,n,atom,'s'|'p',k,[dt,idt,tau,damp],A,B,atom floats 0x18..0xd8 ('s' only)]
//      at the entry of the set-up (0x553c30) or a pass (0x5547d0): A/B are
//      [com3,quat4(w,x,y,z),v3,w3,invmass,invI 9] at the set-up, [v3,w3] at a pass
//   WE [WE,n,atom,'s'|'p',k,[vA3,wA3],[vB3,wB3]]  at its exit
//   ST [ST,n,atom,theta,caller]                 0x553af0: the steered axle rebuilt
//   B  [B,n,[[body,com3,quat4,v3,w3],...]]      every watched body at each stepTo entry
'use strict';
const WHEEL_STEPS = 2400;
let n = 0, rows = [], left = 0;
function push(r) { rows.push(r); if (rows.length >= 512) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const r6 = x => Math.round(x * 1e6) / 1e6;
const v3 = p => [r6(f(p)), r6(f(p.add(4))), r6(f(p.add(8)))];
const list = (p) => { const b = p.readPointer(), e = p.add(4).readPointer(), o = [];
  if (b.isNull()) return o; for (let r = b; r.compare(e) < 0 && o.length < 64; r = r.add(4)) o.push(r.readPointer()); return o; };
const fa = a => new Float32Array(new Int32Array([a.toInt32()]).buffer)[0];
const watch = new Map(), systems = new Set();
Interceptor.attach(ptr('0x534050'), { onEnter() {
  const s = this.context.ecx;
  try {
    const bodies = list(s.add(0x44));
    if (!bodies.some(b => f(b.add(0x190)) >= 20)) return;
    const fresh = bodies.filter(b => !watch.has(b.toString()));
    if (!systems.has(s.toString()) || fresh.length) {
      systems.add(s.toString());
      bodies.forEach(b => watch.set(b.toString(), b));
      push(['Y', s.toString(), bodies.map(b => [b.toString(), f(b.add(0x190))])]);
    }
  } catch (e) {}
} });
const full = b => [v3(b.add(0x30)), [r6(f(b.add(0x40))), r6(f(b.add(0x44))), r6(f(b.add(0x48))), r6(f(b.add(0x4c)))],
  v3(b.add(0x54)), v3(b.add(0x64)), r6(f(b.add(0x98))),
  [].concat(v3(b.add(0x10c)), v3(b.add(0x11c)), v3(b.add(0x12c)))];
const vel = b => [].concat(v3(b.add(0x54)), v3(b.add(0x64)));
const massSeen = new Map();
Interceptor.attach(ptr('0x52aa60'), { onEnter() {
  n++;
  for (const [k, b] of watch) {
    const m = f(b.add(0x190)), fl = b.add(0x1c).readU32(), key = m + ' ' + fl;
    if (massSeen.get(k) !== key) {
      const old = massSeen.get(k);
      massSeen.set(k, key);
      push(['M', n, k, m, fl]);
      if (old !== undefined && m >= 100 && (parseInt(old.split(' ')[1]) & 4) && !(fl & 4)) {
        left = Math.max(left, WHEEL_STEPS); push(['BELL', n, k]);
      }
    }
  }
  if (left > 0) {
    left--;
    const o = [];
    for (const [k, b] of watch) { const s = full(b); o.push([k, s[0], s[1], s[2], s[3]]); }
    push(['B', n, o]);
  }
} });
// The wheel constraint: set-up and passes.
let kS = 0, kP = 0, lastN = -1;
function wheelEnter(ctx, kind, args) {
  if (left <= 0) return null;
  if (n !== lastN) { lastN = n; kS = 0; kP = 0; }
  const a = ctx.ecx, ba = a.add(0xc).readPointer(), bb = a.add(0x10).readPointer();
  const info = args[0];
  let p = [];
  try { p = [r6(f(info.add(4))), r6(f(info.add(8))), r6(fa(args[2])), r6(fa(args[3]))]; } catch (e) {}
  const k = kind === 's' ? kS++ : kP++;
  if (kind === 's') {
    const fl = []; for (let i = 0x18; i < 0xd8; i += 4) fl.push(r6(f(a.add(i))));
    push(['WS', n, a.toString(), kind, k, p, full(ba), full(bb), fl,
          [a.add(0x20).readU8(), a.add(0x21).readU8(), a.add(0x3c).readU8(), a.add(0x3d).readU8()]]);
  } else {
    push(['WS', n, a.toString(), kind, k, p, vel(ba), vel(bb)]);
  }
  return [a, ba, bb, k];
}
for (const [addr, kind] of [['0x553c30', 's'], ['0x5547d0', 'p']]) {
  Interceptor.attach(ptr(addr), {
    onEnter(args) { this.w = wheelEnter(this.context, kind, args); },
    onLeave() { if (this.w) { const [a, ba, bb, k] = this.w; push(['WE', n, a.toString(), kind, k, vel(ba), vel(bb)]); } },
  });
}
Interceptor.attach(ptr('0x553af0'), { onEnter(args) {
  if (left <= 0) return;
  push(['ST', n, this.context.ecx.toString(), r6(fa(args[0])), this.returnAddress.toString()]);
} });
// Who wakes a watched body (setActive, 0x523230).
Interceptor.attach(ptr('0x523230'), { onEnter(a) {
  const b = this.context.ecx, k = b.toString();
  if (!watch.has(k)) return;
  push(['A', n, k, a[0].toInt32() & 0xff, a[1].toInt32() & 0xff, b.add(0x1c).readU32(), f(b.add(0x190)), this.returnAddress.toString()]);
} });
// The detector's wake pass (0x548da0, thiscall (pairs)): the pairs with a body
// it would wake. A pair is 0x74 bytes, its collidables at +8 and +0xc, each
// collidable's body at +0x1c.
const wakeable = fl => (fl & 7) === 0;
Interceptor.attach(ptr('0x548da0'), { onEnter(args) {
  try {
    const lst = args[0], b = lst.add(4).readPointer(), e = lst.add(8).readPointer();
    const o = [];
    for (let p = b; p.compare(e) < 0 && o.length < 32; p = p.add(0x74)) {
      const ca = p.add(8).readPointer(), cb = p.add(0xc).readPointer();
      const ba = ca.add(0x1c).readPointer(), bb = cb.add(0x1c).readPointer();
      const fa_ = ba.add(0x1c).readU32(), fb = bb.add(0x1c).readU32();
      if (!wakeable(fa_) && !wakeable(fb)) continue;
      o.push([p.toString(), ca.toString(), cb.toString(), ba.toString(), bb.toString(), fa_, fb,
              r6(f(ba.add(0x190))), r6(f(bb.add(0x190))), v3(ba.add(0x30)), v3(bb.add(0x30))]);
    }
    if (o.length) push(['DW', n, this.returnAddress.toString(), o]);
  } catch (e) { push(['DWERR', n, e.toString()]); }
} });
// 0x548ef0 (thiscall (body)): the detector wakes a body and then the pairs
// it held back for it (its list +0x78..+0x7c, two collidables each, a
// collidable's body at +0x1c); every call, with the caller, the body and the
// held-back pairs that hold it.
const bstate = b => [b.toString(), b.add(0x1c).readU32(), r6(f(b.add(0x190))), v3(b.add(0x30))];
Interceptor.attach(ptr('0x548ef0'), { onEnter(args) {
  try {
    const b = args[0], det = this.context.ecx;
    const lb = det.add(0x78).readPointer(), le = det.add(0x7c).readPointer();
    const held = [];
    if (!lb.isNull()) for (let p = lb; p.compare(le) < 0 && held.length < 32; p = p.add(8)) {
      const ba = p.readPointer().add(0x1c).readPointer(), bb = p.add(4).readPointer().add(0x1c).readPointer();
      if (ba.equals(b) || bb.equals(b)) held.push([bstate(ba), bstate(bb)]);
    }
    if (!watch.has(b.toString()) && !held.length && !(f(b.add(0x190)) > 0)) return;
    push(['DE', n, bstate(b), this.returnAddress.toString(), held, le.sub(lb).toInt32() >> 3]);
  } catch (e) { push(['DEERR', n, e.toString()]); }
} });
setInterval(flush, 300);
rpc.exports = { flush() { flush(); return n; } };
