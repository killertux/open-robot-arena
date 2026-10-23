// Frida agent: the substeps right after a robot is put on its start point in
// the ORIGINAL, stage by stage (docs/29 section 12x). Section 12w found some
// bodies taking only part of the first substep after the placement; this
// follows every stage of World::stepTo (0x52aa60) for WINDOW calls from each
// placement (0x4dcff0) and dumps every watched body after each stage.
// Loaded before Start Game (capture_rest.py WATCH_AGENT=ra2_pieces.js).
//   Y  [Y,sys,[[body,mass],...]]                  a robot system seen first
//   P  [P,n,'enter'|'leave',this]                 0x4dcff0
//   T  [T,n,target(int,float),world time(int,float)]  stepTo entry
//   S  [S,n,stage,arg,[[body,flags,y,vy,wx,wy,wz,vx,vz],...]]  after a stage:
//      res (vtable+0x24), int (0x52a740, arg dt), zero (0x52a810), det
//      (vtable+0x20, arg its result), act (0x52a990,
//      arg dt), adv (0x4ac980, arg dt); also BELL_WINDOW calls from the bell
//      (a held chassis let go: BELL rows)
'use strict';
const WINDOW = 6, BELL_WINDOW = 40;
let n = 0, rows = [], left = 0;
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const list = (p) => { const b = p.readPointer(), e = p.add(4).readPointer(), o = [];
  if (b.isNull()) return o; for (let r = b; r.compare(e) < 0 && o.length < 64; r = r.add(4)) o.push(r.readPointer()); return o; };
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
const fa = a => new Float32Array(new Int32Array([a.toInt32()]).buffer)[0];
const r4 = x => Math.round(x * 1e5) / 1e5;
function dump(stage, arg) {
  if (left <= 0) return;
  const o = [];
  for (const [k, b] of watch) {
    o.push([k, b.add(0x1c).readU32(), r4(f(b.add(0x34))), r4(f(b.add(0x58))), r4(f(b.add(0x64))), r4(f(b.add(0x68))),
            r4(f(b.add(0x6c))), r4(f(b.add(0x54))), r4(f(b.add(0x5c)))]);
  }
  push(['S', n, stage, arg, o]);
}
let hooked = false, placing = false;
const massSeen = new Map();
Interceptor.attach(ptr('0x4dcff0'), {
  onEnter() {
    left = WINDOW + 1; placing = true; push(['P', n, 'enter', this.context.ecx.toString()]); dump('place', 0);
    // Every watched body's first 0x200 bytes at the placement (the force
    // accumulators among them).
    for (const [k, b] of watch) { const o = []; for (let i = 0; i < 0x200; i += 4) o.push(f(b.add(i))); push(['PB', n, k, o]); }
  },
  onLeave() { placing = false; push(['P', n, 'leave']); },
});
Interceptor.attach(ptr('0x52aa60'), { onEnter(args) {
  n++;
  const w = this.context.ecx;
  // A watched body's mass or flags changed since the last stepTo.
  for (const [k, b] of watch) {
    const m = f(b.add(0x190)), fl = b.add(0x1c).readU32(), key = m + ' ' + fl;
    if (massSeen.get(k) !== key) {
      const old = massSeen.get(k);
      massSeen.set(k, key);
      push(['M', n, k, m, fl]);
      // The bell: a held chassis (+0x1c bit 2) let go opens a window too.
      if (old !== undefined && m >= 100 && (parseInt(old.split(' ')[1]) & 4) && !(fl & 4)) {
        left = Math.max(left, BELL_WINDOW + 1); push(['BELL', n, k]);
      }
    }
  }
  if (!hooked) {
    hooked = true;
    const vt = w.readPointer();
    const res = vt.add(0x24).readPointer(), det = vt.add(0x20).readPointer();
    push(['VT', vt.toString(), res.toString(), det.toString()]);
    Interceptor.attach(res, { onLeave() { dump('res', 0); } });
    Interceptor.attach(det, { onLeave(r) { dump('det', r.toInt32() & 0xff); } });
  }
  if (left > 0) {
    left--;
    push(['T', n, [args[1].toInt32(), fa(args[2])],
          [w.add(0x60).readS32(), f(w.add(0x64))]]);
    dump('enter', 0);
  }
} });
Interceptor.attach(ptr('0x52a740'), { onEnter(a) { this.dt = fa(a[0]); }, onLeave() { dump('int', this.dt); } });
Interceptor.attach(ptr('0x52a810'), { onLeave() { dump('zero', 0); } });
Interceptor.attach(ptr('0x52a990'), { onEnter(a) { this.dt = fa(a[0]); }, onLeave() { dump('act', this.dt); } });
Interceptor.attach(ptr('0x4ac980'), { onEnter(a) { this.dt = fa(a[0]); }, onLeave() { dump('adv', this.dt); } });
// The wheel constraint's set-up (0x553c30, a Steering unit's hubs): the atom's
// first 0xd8 bytes the first time and every call while placing or in the window.
//   W  [W,n,atom,bodyA,bodyB,floats 0..0xd8, bytes 0x20,0x21,0x3c,0x3d]
const seenW = new Set();
Interceptor.attach(ptr('0x553c30'), { onEnter() {
  const a = this.context.ecx, k = a.toString();
  if (seenW.has(k) && !placing && left <= 0) return;
  seenW.add(k);
  const fl = []; for (let i = 0; i < 0xd8; i += 4) fl.push(f(a.add(i)));
  push(['W', n, k, a.add(0xc).readPointer().toString(), a.add(0x10).readPointer().toString(), fl,
        [a.add(0x20).readU8(), a.add(0x21).readU8(), a.add(0x3c).readU8(), a.add(0x3d).readU8()]]);
} });
// The other constraints' set-ups (vtable +0x24 of 0x6ac2d4, 0x6ac488,
// 0x6ac524): the atom's first 0xd8 bytes the first time, and in the window.
//   C  [C,n,setup,atom,bodyA,bodyB,floats 0..0xd8]
const seenC = new Set();
for (const setup of ['0x534850', '0x552bb0', '0x555080']) {
  Interceptor.attach(ptr(setup), { onEnter() {
    const a = this.context.ecx, k = a.toString();
    const ba = a.add(0xc).readPointer().toString(), bb = a.add(0x10).readPointer().toString();
    if (!watch.has(ba) && !watch.has(bb)) return;
    if (seenC.has(k) && left <= 0) return;
    seenC.add(k);
    const fl = []; for (let i = 0; i < 0xd8; i += 4) fl.push(f(a.add(i)));
    push(['C', n, setup, k, ba, bb, fl]);
  } });
}
// The hinge's set-up (0x535a50), as ra2_spinup.js's J rows, on a watched body.
const seenJ = new Set();
Interceptor.attach(ptr('0x535a50'), { onEnter() {
  const a = this.context.ecx, k = a.toString();
  const ba = a.add(0xc).readPointer().toString(), bb = a.add(0x10).readPointer().toString();
  if (!watch.has(ba) && !watch.has(bb)) return;
  if (seenJ.has(k) && left <= 0) return;
  seenJ.add(k);
  const fl = []; for (let i = 0; i < 0xa0; i += 4) fl.push(f(a.add(i)));
  push(['J', n, k, ba, bb, fl]);
} });
// Who wakes a watched body (setActive, 0x523230: thiscall (active, forget)),
// with a short backtrace the first time per caller.
const wakeSeen = new Set();
Interceptor.attach(ptr('0x523230'), { onEnter(a) {
  const b = this.context.ecx, k = b.toString();
  if (!watch.has(k)) return;
  const ra = this.returnAddress.toString();
  let bt = [];
  if (!wakeSeen.has(ra)) { wakeSeen.add(ra); try { bt = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 8).map(x => x.toString()); } catch (e) {} }
  push(['A', n, k, a[0].toInt32() & 0xff, a[1].toInt32() & 0xff, b.add(0x1c).readU32(), f(b.add(0x190)), ra, bt]);
} });
setInterval(flush, 300);
rpc.exports = { flush() { flush(); return n; } };
