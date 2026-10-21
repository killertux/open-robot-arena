// Frida agent: the ORIGINAL's per-hit visual effects (docs/37-effects.md).
// Function-entry hooks only; every hook sends {ev, t, ...}.
//   hit    0x44ee70 entry   component-hit effects: ecx = component, [esp+4] = damage (float),
//                           [esp+8] = contact record (+0x2c = the other component's definition,
//                           +0x80 byte = "has a decal", +0x84 string, +0x94 vec3, +0xa4/+0xa8 w/h, +0xac)
//   base   0x45f3a0 entry   the base class' part of it (sparks + sound), [esp+4] = damage
//   spk    0x4a2a50 entry   spark/debris chooser (ecx = effect manager)
//   spark  0x4a2c10 entry   sparks effect
//   debris 0x4a2e60 entry   metal debris effect
//   puff   0x4a3120 entry   smoke puff (plus.emitSmoke)
//   add    0x4a3380 entry   ParticleManager add (one-shot entry)
//   msmoke 0x48dc50 entry   motor smoke emitter creation
//   smoke2 0x4f6a10 entry   the second smoke emitter creation (class built by 0x4f6280)
//   flame  0x4b5760 entry   plus.AddParticleEmitter
//   decal  0x5a0e00 entry   DecalTrigger constructor
//   emit   0x5bab60 entry   emitter constructor (config at [esp+8])
'use strict';
const t0 = Date.now();
const now = () => Date.now() - t0;
const f = (p, o) => p.add(o).readFloat();
function rp(p) { try { return p.readPointer(); } catch (e) { return null; } }
function vec(p) { try { return [f(p, 0), f(p, 4), f(p, 8)]; } catch (e) { return null; } }
function str(p) {
  // MSVC std::string of this build: char* at +4 (docs/35), fall back to inline text
  try { const q = p.add(4).readPointer(); const s = q.readCString(); if (s && s.length < 200) return s; } catch (e) {}
  try { const s = p.readCString(); if (s && s.length < 200) return s; } catch (e) {}
  return null;
}
function words(sp, n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const a = sp.add(4 + 4 * i);
    const u = a.readU32();
    const fl = a.readFloat();
    const row = { u: u, f: Number.isFinite(fl) && Math.abs(fl) < 1e9 ? +fl.toFixed(5) : null };
    if (u > 0x10000 && u < 0x7fff0000) { const v = vec(ptr(u)); if (v && v.every(x => Number.isFinite(x) && Math.abs(x) < 1e6)) row.v = v.map(x => +x.toFixed(4)); }
    out.push(row);
  }
  return out;
}
function hook(addr, name, n) {
  Interceptor.attach(ptr(addr), { onEnter() {
    try { send({ ev: name, t: now(), ecx: this.context.ecx.toString(), a: words(this.context.esp, n) }); }
    catch (e) { send({ ev: 'err', name: name, e: String(e) }); }
  } });
}
hook('0x45f3a0', 'base', 2);
hook('0x4a2a50', 'spk', 4);
hook('0x4a2c10', 'spark', 6);
hook('0x4a2e60', 'debris', 6);
hook('0x4a3120', 'puff', 6);
hook('0x4a3380', 'add', 6);
hook('0x48dc50', 'msmoke', 2);
hook('0x4f6a10', 'smoke2', 1);
hook('0x4b5760', 'flame', 6);
Interceptor.attach(ptr('0x44ee70'), { onEnter() {
  try {
    const sp = this.context.esp;
    const damage = f(sp, 4), rec = sp.add(8).readPointer();
    const o = { ev: 'hit', t: now(), ecx: this.context.ecx.toString(), damage: damage, rec: rec.toString() };
    o.recf = []; for (let i = 0; i < 16; i++) o.recf.push(+f(rec, 4 * i).toFixed(4));
    const w = rp(rec.add(0x2c));
    if (w && !w.isNull()) {
      o.w = w.toString(); o.flag = w.add(0x80).readU8(); o.file = str(w.add(0x84));
      o.dir = vec(w.add(0x94)); o.size = [f(w, 0xa4), f(w, 0xa8)]; o.mode = w.add(0xac).readS32();
    }
    send(o);
  } catch (e) { send({ ev: 'err', name: 'hit', e: String(e) }); }
} });
Interceptor.attach(ptr('0x5a0e00'), { onEnter() {
  try {
    const sp = this.context.esp;
    const o = { ev: 'decal', t: now(), obj: this.context.ecx.toString() };
    o.file = str(sp.add(4).readPointer());
    o.c = vec(sp.add(8).readPointer()); o.u = vec(sp.add(12).readPointer()); o.v = vec(sp.add(16).readPointer());
    o.w = f(sp, 20); o.h = f(sp, 24); o.depth = f(sp, 28);
    const mesh = sp.add(32).readPointer(); o.mesh = mesh.toString(); o.ntri = mesh.add(0xa0).readS32();
    o.p9 = sp.add(36).readU32(); o.p11 = sp.add(44).readU32(); o.p12 = sp.add(48).readU32();
    o.bias = f(sp, 52); o.alpha = f(sp, 56);
    this.o = o; this.obj = this.context.ecx;
  } catch (e) { send({ ev: 'err', name: 'decal', e: String(e) }); }
}, onLeave() {
  try {
    const o = this.o, d = this.obj;
    o.nv = d.add(0x170).readS32(); o.nt = d.add(0x174).readS32();
    o.tex = d.add(0x1ce4).readU32(); o.flag = d.add(0x1cf0).readU8();
    o.pos = []; o.uv = [];
    const n = Math.min(o.nv, 89);
    for (let i = 0; i < n; i++) {
      o.pos.push([+f(d, 0x178 + 12 * i).toFixed(5), +f(d, 0x17c + 12 * i).toFixed(5), +f(d, 0x180 + 12 * i).toFixed(5)]);
      o.uv.push([+f(d, 0x1258 + 8 * i).toFixed(5), +f(d, 0x125c + 8 * i).toFixed(5)]);
    }
    o.idx = []; for (let i = 0; i < o.nt * 3; i++) o.idx.push(d.add(0x1ac8 + 2 * i).readU16());
    o.bary = []; for (let i = 0; i < n; i++) o.bary.push([d.add(0x9ec + 24 * i - 4).readU32(), d.add(0x9ec + 24 * i).readU32(), d.add(0x9ec + 24 * i + 4).readU32(),
        +f(d, 0x9ec + 24 * i + 8).toFixed(5), +f(d, 0x9ec + 24 * i + 12).toFixed(5), +f(d, 0x9ec + 24 * i + 16).toFixed(5)]);
    send(o);
  } catch (e) { send({ ev: 'err', name: 'decal-leave', e: String(e) }); }
} });
// the emitter config handed to 0x5bab60: rate (+0x38), max (+0x48), life (+0x90..), velocity (+0x98), colours, sizes
Interceptor.attach(ptr('0x5bab60'), { onEnter() {
  try {
    const cfg = this.context.esp.add(8).readPointer();
    const o = { ev: 'emit', t: now(), ret: this.returnAddress.toString() };
    for (const [k, off, n] of [['rate', 0x38, 1], ['max', 0x48, 1], ['fade', 0x4c, 1], ['grav', 0x54, 3], ['life', 0x90, 2],
                               ['vel', 0x98, 3], ['vspread', 0xa4, 3], ['c0', 0xb0, 4], ['c1', 0xc0, 4], ['cs', 0xd0, 4],
                               ['size', 0xe0, 3]]) {
      const r = []; for (let i = 0; i < n; i++) r.push(+f(cfg, off + 4 * i).toFixed(4)); o[k] = r;
    }
    o.flags = [0x00, 0x84, 0xec, 0xed, 0xee].map(x => cfg.add(x).readU8());
    o.mode = cfg.add(0x70).readS32();
    send(o);
  } catch (e) { send({ ev: 'err', name: 'emit', e: String(e) }); }
} });
// The decal placement chain inside one hit (0x44ee70): matrix helpers called while it runs.
let inHit = 0;
Interceptor.attach(ptr('0x44ee70'), { onEnter() { inHit++; }, onLeave() { inHit--; } });
function m12(p) { try { const r = []; for (let i = 0; i < 12; i++) r.push(+f(p, 4 * i).toFixed(5)); return r; } catch (e) { return null; } }
function v3(p) { return vec(p).map(x => +x.toFixed(5)); }
// 0x51eb00(ecx = matrix, [esp+4] = out matrix): rigid inverse
Interceptor.attach(ptr('0x51eb00'), { onEnter() { if (!inHit) return; this.m = this.context.ecx; this.o = this.context.esp.add(4).readPointer(); }, onLeave() {
  if (!this.m) return; send({ ev: 'inv', t: now(), addr: this.m.toString(), m: m12(this.m), out: m12(this.o) }); } });
// 0x51e650(ecx = matrix, [esp+4] = out, [esp+8] = in): out = M * in + t
Interceptor.attach(ptr('0x51e650'), { onEnter() { if (!inHit) return; this.m = this.context.ecx; const sp = this.context.esp; this.o = sp.add(4).readPointer(); this.i = sp.add(8).readPointer(); this.iv = v3(this.i); }, onLeave() {
  if (!this.m) return; send({ ev: 'xf', t: now(), m: m12(this.m), in: this.iv, out: v3(this.o) }); } });
Interceptor.attach(ptr('0x51e6d0'), { onEnter() { if (!inHit) return; this.m = this.context.ecx; const sp = this.context.esp; this.o = sp.add(4).readPointer(); this.i = sp.add(8).readPointer(); this.iv = v3(this.i); }, onLeave() {
  if (!this.m) return; send({ ev: 'rot', t: now(), m: m12(this.m), in: this.iv, out: v3(this.o) }); } });
Interceptor.attach(ptr('0x51e600'), { onEnter() { if (!inHit) return; this.m = this.context.ecx; this.o = this.context.esp.add(4).readPointer(); this.iv = v3(this.o); }, onLeave() {
  if (!this.m) return; send({ ev: 'rot2', t: now(), m: m12(this.m), in: this.iv, out: v3(this.o) }); } });
Interceptor.attach(ptr('0x44cf00'), { onEnter() { if (!inHit) return; const sp = this.context.esp; send({ ev: 'veq', t: now(), a: v3(this.context.ecx), b: v3(sp.add(4).readPointer()) }); } });
rpc.exports = { ping() { return now(); }, glob() { return v3(ptr('0x71abcc')); }, m12at(a) { return m12(ptr(a)); } };
