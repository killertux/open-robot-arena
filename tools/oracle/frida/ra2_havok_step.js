// Frida agent: Havok step / backstep / hinge-motor instrumentation of the ORIGINAL.
// Addresses: see local notes HAVOK_MAP.md.  Events are buffered and sent in batches
// as {ev:'batch', rows:[...]}; each row is an array whose first element is a tag.
//   F  frame        [F,seq,ms,frameDt]                         0x4accb0 enter
//   S  stepTo       [S,seq,ms,world,dtSub(world+0x6c)]         0x52aa60 enter
//   I  integrate    [I,seq,h,rbTotal]                          0x52a740 entry (each integration attempt)
//   A  actions      [A,seq,h,rbTotal]                          0x52a990 entry (once per accepted piece; constraint solver stage)
//   H  hinge setup  [H,seq,atom,bodyA,bodyB,motor,limited,tgtVel,maxT,dt,invdt,tgtPre,tgtPost,theta,
//                    lambda,capped,wA(3),wB(3)]                0x535a50 (+0x5363b0 theta, 0x5364f0 lambda)
//   (H rows: body layout +0x30 pos, +0x54 lin vel, +0x64 ang vel; fields 20-23 = vA,vB,pA,pB)
//   V  hinge solve  [V,seq,atom]                               0x5361e0 enter
'use strict';
// HOOKS is prepended by the host script: a string of tags to enable (default all).
const EN = (typeof HOOKS === 'string') ? HOOKS : 'FSIAVHTLMC';
const on = t => EN.indexOf(t) >= 0;
let seq = 0, rows = [], t0 = Date.now();
function push(r) { rows.push(r); if (rows.length >= 512) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
function vec(p) { return [f(p), f(p.add(4)), f(p.add(8))]; }

if (on('F')) Interceptor.attach(ptr('0x4accb0'), { onEnter() {
  push(['F', ++seq, Date.now() - t0, f(this.context.esp.add(4))]); } });
if (on('S')) Interceptor.attach(ptr('0x52aa60'), { onEnter() {
  const w = this.context.ecx; this.world = w;
  push(['S', ++seq, Date.now() - t0, w.toString(), f(w.add(0x6c))]); } });
// Mid-function hooks (even at x87-empty points) made the game's physics explode: only
// function entries are hooked.  Integration 0x52a740 / Actions 0x52a990 are thiscall(world, h).
// rb = cumulative "Backstep/Roll Back" counter (DAT_00719724).
if (on('I')) Interceptor.attach(ptr('0x52a740'), { onEnter() {
  push(['I', ++seq, f(this.context.esp.add(4)), ptr('0x719724').readU32()]); } });
if (on('A')) Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', ++seq, f(this.context.esp.add(4)), ptr('0x719724').readU32()]); } });
if (on('V')) Interceptor.attach(ptr('0x5361e0'), { onEnter() { push(['V', ++seq, this.context.ecx.toString()]); } });

// hinge setup
// HingeController (game side): 0x481450 motor step (thiscall ctl), 0x4815c0 set motor (vel, torque)
// M [M,seq,ctl,hinge,pct(+0x7c),ctl+0xbc,ctl+0xb8,blk motorOn(+0x91),blk maxT(+0xa0),blk vel(+0xa4)]
if (on('M')) Interceptor.attach(ptr('0x481450'), { onEnter() {
  const c = this.context.ecx; let h = ptr(0);
  try { h = c.add(0x58).readPointer(); push(['M', ++seq, c.toString(), h.toString(), c.add(0x7c).readS32(),
    f(c.add(0xbc)), f(c.add(0xb8)), h.add(0x91).readU8(), f(h.add(0xa0)), f(h.add(0xa4))]); } catch (e) {} } });
if (on('C')) Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  const c = this.context.ecx; const sp = this.context.esp;
  push(['C', ++seq, c.toString(), f(sp.add(4)), f(sp.add(8)), sp.add(4).readU32(), sp.add(8).readU32()]); } });
// Wheel-constraint (type 5) probes: [W,seq,addr,ecx,u32 x4 stack words]
if (on('W')) ['0x52b630', '0x552bb0', '0x553c30', '0x5533c0', '0x5547d0'].forEach(a => Interceptor.attach(ptr(a), { onEnter() {
  const sp = this.context.esp;
  push(['W', ++seq, a, this.context.ecx.toString(), sp.add(4).readU32(), sp.add(8).readU32(), sp.add(12).readU32(), sp.add(16).readU32()]); } }));
let cur = null, dumpN = 0;
function hex(p) { try { return Array.from(new Uint8Array(p.readByteArray(0x1c0))).map(b => ('0' + b.toString(16)).slice(-2)).join(''); } catch (e) { return null; } }
if (on('H')) Interceptor.attach(ptr('0x535a50'), {
  onEnter(args) {
    const a = this.context.ecx, info = this.context.esp.add(4).readPointer();
    const bA = a.add(0xc).readPointer(), bB = a.add(0x10).readPointer();
    cur = { atom: a, bA: bA, bB: bB, motor: a.add(0x81).readU8(), lim: a.add(0x80).readU8(),
            vel: f(a.add(0x94)), maxT: f(a.add(0x90)), dt: f(info.add(4)), inv: f(info.add(8)),
            tgtPre: f(a.add(0x98)), theta: NaN, lam: 0, capped: 0,
            wA: vec(bA.add(0x64)), wB: vec(bB.add(0x64)),
            vA: vec(bA.add(0x54)), vB: vec(bB.add(0x54)), pA: vec(bA.add(0x30)), pB: vec(bB.add(0x30)) };
    if (cur.motor && (++dumpN % 64) === 0) { cur.dA = hex(bA); cur.dB = hex(bB); }
    this.c = cur;
  },
  onLeave() {
    const c = this.c; if (!c) return;
    push(['H', ++seq, c.atom.toString(), c.bA.toString(), c.bB.toString(), c.motor, c.lim, c.vel, c.maxT,
          c.dt, c.inv, c.tgtPre, f(c.atom.add(0x98)), c.theta, c.lam, c.capped, c.wA, c.wB, c.dA || null, c.dB || null, c.vA, c.vB, c.pA, c.pB]);
    cur = null;
  } });
// x87-safe: only function entries are hooked in the FPU-heavy setup (mid-function hooks there
// crash the game: Frida does not preserve the x87 stack).  theta is read from the caller's
// stack in the call 0x535c51 -> 0x5363b0 (x87 stack empty at call boundaries); lambda from
// the first call 0x535e6a -> 0x5364f0.  "capped" is inferred from the target-angle slip.
if (on('T')) Interceptor.attach(ptr('0x5363b0'), { onEnter() {
  if (cur && this.returnAddress.equals(ptr('0x535c56'))) cur.theta = f(this.context.esp.add(0x14)); } });
if (on('L')) Interceptor.attach(ptr('0x5364f0'), { onEnter() {
  if (cur && this.returnAddress.equals(ptr('0x535e6f'))) cur.lam = f(this.context.esp.add(12).readPointer()); } });

rpc.exports = { flush() { flush(); return seq; },
  world(w) { const p = ptr(w); return { grav: [f(p.add(0x1c)), f(p.add(0x20)), f(p.add(0x24)), f(p.add(0x28))],
    dt: f(p.add(0x6c)), det: p.add(0x78).readPointer().toString() }; },
  read(addr, n) { return Memory.readByteArray(ptr(addr), n); } };
setInterval(flush, 500);
