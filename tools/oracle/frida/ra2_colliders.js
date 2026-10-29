// Frida agent: every fixed (massless) collidable the resolver is handed a contact
// for in the ORIGINAL (docs/29 sections 12h, 12i). rpc get() returns, per collidable,
// its shape class (+0x18), AABB (+0x20 min, +0x30 max, grown by the 0.05 radius),
// header words +0x48..+0x54 (+0x4c is a mesh's vertex count), the number of contacts
// and the contact points' extent.  Driver: python with frida, load, wait, call get().
'use strict';
const seen = {};
const f = p => p.readFloat();
Interceptor.attach(ptr('0x53ed80'), { onEnter() {
  try {
    const list = this.context.esp.add(4).readPointer();
    const b = list.add(4).readPointer(), e = list.add(8).readPointer();
    for (let r = b; r.compare(e) < 0; r = r.add(0x74)) {
      for (const off of [8, 0xc]) {
        const c = r.add(off).readPointer();
        const body = c.add(0x1c).readPointer();
        if (f(body.add(0x190)) !== 0) continue;
        const k = c.toString();
        if (!seen[k]) seen[k] = { type: c.add(0x18).readU32(), min: [f(c.add(0x20)), f(c.add(0x24)), f(c.add(0x28))],
          max: [f(c.add(0x30)), f(c.add(0x34)), f(c.add(0x38))], w: [0x48,0x4c,0x50,0x54].map(o => c.add(o).readU32()), n: 0,
          lo: [1e9,1e9,1e9], hi: [-1e9,-1e9,-1e9] };
        const s = seen[k]; s.n++;
        for (let i = 0; i < 3; i++) { const v = f(r.add(0x20 + 4 * i)); s.lo[i] = Math.min(s.lo[i], v); s.hi[i] = Math.max(s.hi[i], v); }
      }
    }
  } catch (err) {}
} });
rpc.exports = { get() { return seen; } };
