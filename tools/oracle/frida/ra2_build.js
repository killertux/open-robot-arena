// Frida agent: the detector's held-back pairs during the robots' build in the
// ORIGINAL (docs/29 section 12z). Loaded before Start Game
// (capture_build.py, through ra2_hook.py); logs the first NMAX stepTo calls.
//   N  [N,n]                                    every 200th stepTo (0x52aa60)
//   HB [HB,n,X,[[ca,cb,A,B,boxA,boxB,rec floats +0x10..+0x70],...],count]
//      0x548ab0 (thiscall (body X)): X goes inactive (0x52b290(X, 0)); the
//      detector's pair list +0x144..+0x148 (0x74-byte records, collidables at
//      +8/+0xc, a collidable's body at +0x1c) is gone through and every pair
//      of X's with an inactive partner is held back. Every pair of X's in the
//      list is logged, with both bodies and both collidables' boxes
//      (+0x20/+0x30).
//   WK [WK,n,X,caller,[[ca,cb,A,B,boxA,boxB],...]]  0x548ef0 (thiscall (body)):
//      the detector wakes X and goes through the held pairs with it
//   AC [AC,n,body,active,forget,caller,state]   setActive (0x523230)
// A body is [ptr, flags +0x1c, mass +0x190, com +0x30, quat +0x40 (w x y z)].
'use strict';
const NMAX = 9000;
let n = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 512) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const r6 = x => Math.round(x * 1e6) / 1e6;
const v3 = p => [r6(f(p)), r6(f(p.add(4))), r6(f(p.add(8)))];
const body = b => [b.toString(), b.add(0x1c).readU32(), r6(f(b.add(0x190))), v3(b.add(0x30)),
  [r6(f(b.add(0x40))), r6(f(b.add(0x44))), r6(f(b.add(0x48))), r6(f(b.add(0x4c)))]];
const box = c => [v3(c.add(0x20)), v3(c.add(0x30))];
Interceptor.attach(ptr('0x52aa60'), { onEnter() { n++; if (n % 200 === 0) push(['N', n]); } });
Interceptor.attach(ptr('0x548ab0'), { onEnter(args) {
  if (n > NMAX) return;
  try {
    const X = args[0], det = this.context.ecx;
    const b = det.add(0x144).readPointer(), e = det.add(0x148).readPointer();
    const o = [];
    let count = 0;
    if (!b.isNull()) for (let r = b; r.compare(e) < 0 && count < 4000; r = r.add(0x74), count++) {
      const ca = r.add(8).readPointer(), cb = r.add(0xc).readPointer();
      const ba = ca.add(0x1c).readPointer(), bb = cb.add(0x1c).readPointer();
      if (!ba.equals(X) && !bb.equals(X)) continue;
      const fl = []; for (let i = 0x10; i < 0x70; i += 4) fl.push(r6(f(r.add(i))));
      o.push([ca.toString(), cb.toString(), body(ba), body(bb), box(ca), box(cb), fl]);
    }
    push(['HB', n, body(X), o, count]);
  } catch (err) { push(['ERR', n, 'HB ' + err]); }
} });
Interceptor.attach(ptr('0x548ef0'), { onEnter(args) {
  if (n > NMAX) return;
  try {
    const X = args[0], det = this.context.ecx;
    const lb = det.add(0x78).readPointer(), le = det.add(0x7c).readPointer();
    const o = [];
    if (!lb.isNull()) for (let p = lb; p.compare(le) < 0 && o.length < 64; p = p.add(8)) {
      const ca = p.readPointer(), cb = p.add(4).readPointer();
      const ba = ca.add(0x1c).readPointer(), bb = cb.add(0x1c).readPointer();
      if (!ba.equals(X) && !bb.equals(X)) continue;
      o.push([ca.toString(), cb.toString(), body(ba), body(bb), box(ca), box(cb)]);
    }
    push(['WK', n, body(X), this.returnAddress.toString(), o]);
  } catch (err) { push(['ERR', n, 'WK ' + err]); }
} });
Interceptor.attach(ptr('0x523230'), { onEnter(a) {
  if (n > NMAX) return;
  try {
    const b = this.context.ecx;
    push(['AC', n, b.toString(), a[0].toInt32() & 0xff, a[1].toInt32() & 0xff, this.returnAddress.toString(), body(b)]);
  } catch (err) { push(['ERR', n, 'AC ' + err]); }
} });
setInterval(flush, 300);
rpc.exports = { flush() { flush(); return n; } };
