// Frida agent: poll the ORIGINAL's main scene camera (game object +0x2a610).
// Camera fields (see docs/30): +0x8c eye (current), +0x1c8 eye (goal), +0x19c rot (pitch,yaw,roll),
// +0x180 fov, +0x1a8 target object ptr (its position at +0x8c), +0x290 range-enabled,
// +0x294..0x2a0 fov range (min_d, max_d, fov_near, fov_far).
// Captures the camera pointer from the setCameraPosition / projection-rebuild entries.
'use strict';
let cam = null;
const hook = a => Interceptor.attach(ptr(a), { onEnter() { if (a === '0x5a7460' || a === '0x5a6130') cam = this.context.ecx; } });
hook('0x5a7460'); hook('0x5a6130'); hook('0x5a6180');
const f = (p, o) => p.add(o).readFloat();
const v3 = (p, o) => [f(p, o), f(p, o + 4), f(p, o + 8)];
const t0 = Date.now();
setInterval(() => {
  if (cam === null) return;
  try {
    const tp = cam.add(0x1a8).readPointer();
    let tgt = null;
    if (!tp.isNull()) tgt = v3(tp, 0x8c);
    send({ t: Date.now() - t0, eye: v3(cam, 0x8c), eyeGoal: v3(cam, 0x1c8), rot: v3(cam, 0x19c),
           fov: f(cam, 0x180), tptr: tp.toString(), tgt: tgt, range: [cam.add(0x290).readU8()].concat([0x294, 0x298, 0x29c, 0x2a0].map(o => f(cam, o))),
           mode: cam.add(0x214).readU32(), cam: cam.toString() });
  } catch (e) { send({ err: e.toString() }); }
}, 250);
