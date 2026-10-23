// Frida agent: the arenas' prismatic controllers in the ORIGINAL (docs/29
// section 12y): every update (0x4b5240), push (0x4b51b0) and lock (0x4b50c0)
// of a script's AddPrismatic joint (controller vtable 0x6a91b8).
//   T  [T,n]                                              every 120th stepTo (0x52aa60)
//   U  [U,n,ctrl,dir,stop6c,stop70,speed,force,locked,autolock,lim64,lim68,[[where,offset,com3,v3],...]]
//   BODIES [BODIES,n,ctrl,[[where,offset,body,mass],...]]  the candidate moving bodies, once
//      at the update's entry (dir +0x80, stops +0x6c/+0x70, power +0x74/+0x78,
//      lock +0x7c, auto-lock +0x7d, travel +0x64/+0x68, the moving body +0x5c)
//   F  [F,n,ctrl,force,caller]                            0x4b51b0 (one push)
//   L  [L,n,ctrl,on,caller]                               0x4b50c0 (Lock)
'use strict';
let n = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 512) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const r5 = x => Math.round(x * 1e5) / 1e5;
const v3 = p => [r5(f(p)), r5(f(p.add(4))), r5(f(p.add(8)))];
const fa = a => new Float32Array(new Int32Array([a.toInt32()]).buffer)[0];
const rawSeen = new Set(), bodyOf = new Map();
const isCtrl = c => { try { return c.readPointer().equals(ptr('0x6a91b8')); } catch (e) { return false; } };
Interceptor.attach(ptr('0x52aa60'), { onEnter() { n++; if (n % 120 === 0) push(['T', n]); } });
Interceptor.attach(ptr('0x4b5240'), { onEnter() {
  const c = this.context.ecx;
  if (!isCtrl(c)) return;
  // The moving body: the Havok constraint (+0x58) holds its two bodies at
  // +0xc and +0x10 (as the constraint atoms do); logged with its mass.
  // The moving body: searched once among the pointers of the controller and
  // of the constraint it holds (+0x58), and through a pointer's +0x1c (a
  // collidable's body), for a rigid body with a sane mass (+0x190).
  let bb = [];
  try {
    if (!bodyOf.has(c.toString())) {
      const cons = c.add(0x58).readPointer();
      const found = [];
      const sane = b => { try { const m = f(b.add(0x190)); return m > 0.5 && m < 1e5 ? m : 0; } catch (e) { return 0; } };
      for (const [base, nm] of [[c, 'c'], [cons, 'k']]) {
        for (let i = 0; i < 0x80; i += 4) {
          let p; try { p = base.add(i).readPointer(); } catch (e) { continue; }
          if (p.compare(ptr('0x10000')) < 0) continue;
          const m = sane(p); if (m) found.push([nm, i, p, m]);
          try { const q = p.add(0x1c).readPointer(); const m2 = sane(q); if (m2) found.push([nm + '+1c', i, q, m2]); } catch (e) {}
        }
      }
      bodyOf.set(c.toString(), found);
      push(['BODIES', n, c.toString(), found.map(x => [x[0], x[1], x[2].toString(), r5(x[3])])]);
    }
    for (const x of bodyOf.get(c.toString())) bb.push([x[0], x[1], v3(x[2].add(0x30)), v3(x[2].add(0x54))]);
  } catch (e) { bb = [String(e)]; }
  push(['U', n, c.toString(), c.add(0x80).readS32(), r5(f(c.add(0x6c))), r5(f(c.add(0x70))), r5(f(c.add(0x74))),
        r5(f(c.add(0x78))), c.add(0x7c).readU8(), c.add(0x7d).readU8(), r5(f(c.add(0x64))), r5(f(c.add(0x68))), bb]);
} });
Interceptor.attach(ptr('0x4b51b0'), { onEnter(a) {
  const c = this.context.ecx;
  if (!isCtrl(c)) return;
  push(['F', n, c.toString(), r5(fa(a[0])), this.returnAddress.toString()]);
} });
Interceptor.attach(ptr('0x4b50c0'), { onEnter(a) {
  const c = this.context.ecx;
  if (!isCtrl(c)) return;
  push(['L', n, c.toString(), a[0].toInt32() & 0xff, this.returnAddress.toString()]);
} });
setInterval(flush, 300);
rpc.exports = { flush() { flush(); return n; } };
