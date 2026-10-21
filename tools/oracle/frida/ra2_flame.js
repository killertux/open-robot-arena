// Frida agent: the ORIGINAL's Flamethrower component (docs/37 section 6).
//   fire   0x504a70 entry  Flamethrower "Fire" handler: ecx = component, [esp+4] on flag, [esp+12] level
//   burst  0x504b60 entry  "Fire burst" handler
//   nozzle 0x504a50 entry  sets the emitter's world velocity (+0x180); ecx = emitter, [esp+4] -> vec3
//   hit    0x504ca0 entry  the flame particle's collision callback ([esp+16] = hit record)
//   dmg    0x4175b0 entry  the damage sink (as ra2_damage.js)
//   energy 0x504c50 entry  the per-step draw (dt in [esp+4]); out pointers [esp+8], [esp+12]
//   emit   0x5bab60 entry  emitter constructor, config dump
//   tick   every 50 ms: the Flamethrower object's fields 0x280.. and the emitter's emitting flag
'use strict';
const t0 = Date.now();
const now = () => Date.now() - t0;
const f = (p, o) => p.add(o).readFloat();
function vec(p) { try { return [f(p, 0), f(p, 4), f(p, 8)].map(x => +x.toFixed(5)); } catch (e) { return null; } }
function words(sp, n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const a = sp.add(4 + 4 * i);
    const u = a.readU32(); const fl = a.readFloat();
    out.push({ u: u, f: Number.isFinite(fl) && Math.abs(fl) < 1e9 ? +fl.toFixed(5) : null });
  }
  return out;
}
let flame = null;
// The handlers are called the way the control board calls them, from the agent's own thread (a call
// made inside another hook's callback is invisible to the hooks, which is also why `Input` issued
// by ra2_python.py shows no `fire` row).
const fireFn = new NativeFunction(ptr('0x504a70'), 'int8', ['pointer', 'int8', 'int32'], { abi: 'thiscall' });
const burstFn = new NativeFunction(ptr('0x504b60'), 'int8', ['pointer', 'int8', 'int32'], { abi: 'thiscall' });
function hook(addr, name, n, extra) {
  Interceptor.attach(ptr(addr), { onEnter() {
    try {
      const o = { ev: name, t: now(), ecx: this.context.ecx.toString(), a: words(this.context.esp, n) };
      if (extra) extra.call(this, o);
      send(o);
    } catch (e) { send({ ev: 'err', name: name, e: String(e) }); }
  } });
}
Interceptor.attach(ptr('0x504790'), { onEnter() { flame = this.context.ecx; } });
hook('0x504a70', 'fire', 3, function (o) { flame = this.context.ecx; });
hook('0x504b60', 'burst', 4, function (o) { flame = this.context.ecx; });
hook('0x504a50', 'nozzle', 1, function (o) {
  const p = this.context.esp.add(4).readPointer(); o.vel = vec(p);
});
hook('0x504ca0', 'hit', 6, function (o) {
  const p = this.context.esp.add(4 + 16).readPointer(); o.rec = p.toString();
  try { o.recf = []; for (let i = 0; i < 12; i++) o.recf.push(+f(p, 4 * i).toFixed(4)); } catch (e) {}
});
hook('0x4175b0', 'dmg', 8, function (o) {
  const sp = this.context.esp;
  o.from = this.returnAddress.toString();
  o.v5 = vec(sp.add(4 + 20).readPointer());
});
hook('0x504c50', 'energy', 3);
// the nozzle's local point to the world (0x5048e0 calls 0x51e590(matrix, &out) at 0x50497a)
let nzl = 0;
Interceptor.attach(ptr('0x51e590'), {
  onEnter(args) {
    if (this.returnAddress.toInt32() !== 0x50497f) return;
    this.m = this.context.ecx; this.out = args[0];
  },
  onLeave() {
    if (!this.m || nzl > 40) return;
    nzl++;
    const o = { ev: 'nzl', t: now(), m: [] };
    for (let i = 0; i < 16; i++) o.m.push(+f(this.m, 4 * i).toFixed(4));
    o.out = vec(this.out);
    send(o);
  }
});
hook('0x4f0000', 'snd_create', 8, function (o) {
  try { const nm = this.context.esp.add(8).readPointer(); o.name = nm.add(4).readPointer().readCString(); } catch (e) {}
});
hook('0x4f0680', 'snd_loop', 1);
hook('0x4f05c0', 'snd_stop', 1);
hook('0x4f0bb0', 'snd_vol', 3);
Interceptor.attach(ptr('0x5bab60'), { onEnter() {
  try {
    const cfg = this.context.esp.add(8).readPointer();
    const o = { ev: 'emit', t: now(), cfg: [] };
    for (let i = 0; i < 0x1e0; i += 4) o.cfg.push(+f(cfg, i).toFixed(5));
    send(o);
  } catch (e) { send({ ev: 'err', name: 'emit', e: String(e) }); }
} });
Interceptor.attach(ptr('0x5045b0'), {
  onEnter(args) { this.e = this.context.ecx; this.stage = args[0].toInt32(); },
  onLeave() { try { send({ ev: 'init', t: now(), stage: this.stage, snd: this.e.add(0x2a8).readS32(), em: this.e.add(0x2a4).readPointer().toString(), obj: this.e.toString() }); } catch (e) { send({ ev: 'err', name: 'init', e: String(e) }); } }
});
setInterval(() => {
  if (!flame) return;
  try {
    const em = flame.add(0x2a4).readPointer();
    const o = { ev: 'tick', t: now(), firing: flame.add(0x280).readU8(), burst: flame.add(0x281).readU8(),
      timer: +f(flame, 0x284).toFixed(4), level: flame.add(0x2a0).readS32(), scale: +f(flame, 0x29c).toFixed(4),
      dir: vec(flame.add(0x290)), emitting: em.add(0x324).readU8(), snd: flame.add(0x2a8).readS32(),
      epos: vec(em.add(0x174)), evel: vec(em.add(0x180)) };
    send(o);
    if (o.emitting && !globalThis.dumped) {
      globalThis.dumped = true;
      const d = { ev: 'edump', t: now(), f: [] };
      for (let i = 0x100; i < 0x340; i += 4) d.f.push(+f(em, i).toFixed(4));
      d.bytes = {};
      for (const o of [0xe8, 0xec, 0xed, 0xee, 0x1d4, 0x1d5, 0x1d6, 0x1d7, 0x324, 0x325, 0x326, 0x327]) d.bytes[o.toString(16)] = em.add(o).readU8();
      send(d);
    }
  } catch (e) { send({ ev: 'err', name: 'tick', e: String(e) }); }
}, 50);
rpc.exports = {
  flame() { return flame ? flame.toString() : null; },
  fire(on, level) { fireFn(flame, on, level); },
  burst(on, level) { burstFn(flame, on, level); },
  g() { return [f(ptr('0x714a6c'), 0), f(ptr('0x714a70'), 0)]; },
};
