// Frida agent: every cast the ORIGINAL's flame particles make (docs/37 section 6.2).
//   step  0x5bdc00 entry/leave  one particle step of the jet emitter: ecx = emitter, three stack args (read as vec3
//         through the pointer, and raw)
//   wrap  0x5be380 entry/leave  the cast wrapper: [ecx+0x32c] gates the call of [ecx+0x334] with [ecx+0x330]
//   cb    0x504b90 entry/leave  the Flamethrower's cast callback (its own robot is skipped)
//   ray   0x4a3a90 entry/leave  the world cast; 0x416e30, 0x414800 the body -> robot / component lookups
// Every row carries the step number `k` it happened under, so a cast is tied to its particle step.
'use strict';
const t0 = Date.now();
const f = (p, o) => p.add(o).readFloat();
const r5 = v => +v.toFixed(5);
let on = false;
let k = 0;
let seq = 0;
let jet = null;
let emitterOf = null;
function vec(p) { try { return [r5(f(p, 0)), r5(f(p, 4)), r5(f(p, 8))]; } catch (e) { return null; } }
function words(sp, n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const a = sp.add(4 + 4 * i);
    let v = null;
    try { const q = a.readPointer(); v = vec(q); } catch (e) {}
    out.push({ u: a.readU32(), f: (() => { const x = a.readFloat(); return Number.isFinite(x) && Math.abs(x) < 1e9 ? r5(x) : null; })(), v: v });
  }
  return out;
}
function hook(addr, name, n) {
  Interceptor.attach(ptr(addr), {
    onEnter() {
      if (!on || k === 0) return;
      this.ok = true;
      this.a = [];
      for (let i = 0; i < n; i++) this.a.push(this.context.esp.add(4 + 4 * i).readPointer());
      send({ ev: 'c_' + name, ph: 'in', k: k, t: Date.now() - t0, ecx: this.context.ecx.toString(), a: words(this.context.esp, n) });
    },
    onLeave(ret) {
      if (!this.ok) return;
      const w = this.a.map(p => { try { return vec(p); } catch (e) { return null; } });
      send({ ev: 'c_' + name, ph: 'out', k: k, eax: ret.toInt32(), al: ret.toInt32() & 255, v: w });
    }
  });
}
// the jet's emitter: learnt from the flame object's tick (0x504790) like ra2_flame.js
let flame = null;
Interceptor.attach(ptr('0x504790'), { onEnter() { flame = this.context.ecx; } });
const isJet = ecx => { try { return flame && ecx.equals(flame.add(0x2a4).readPointer()); } catch (e) { return false; } };
Interceptor.attach(ptr('0x5bdc00'), {
  onEnter() {
    if (!on || !isJet(this.context.ecx)) { this.skip = true; return; }
    k = ++seq;
    this.args = [0, 1, 2].map(i => this.context.esp.add(4 + 4 * i).readPointer());
    send({ ev: 'c_step', ph: 'in', k: k, t: Date.now() - t0, wall: Date.now(), a: words(this.context.esp, 3),
      grav: [r5(f(this.context.ecx, 0x13c)), r5(f(this.context.ecx, 0x140)), r5(f(this.context.ecx, 0x144))],
      flags: [this.context.ecx.add(0x326).readU8(), this.context.ecx.add(0x1d6).readU8()] });
  },
  onLeave() {
    if (this.skip) return;
    send({ ev: 'c_step', ph: 'out', k: k, v: this.args.map(p => vec(p)) });
    k = 0; // casts outside a jet step are not logged
  }
});
hook('0x5be380', 'wrap', 5);
hook('0x504b90', 'cb', 5);
hook('0x4a3a90', 'ray', 4);
hook('0x416e30', 'who', 2);
hook('0x414800', 'comp', 2);
rpc.exports = { on(v) { on = !!v; k = 0; return on; } };
