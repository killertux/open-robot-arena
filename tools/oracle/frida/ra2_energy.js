// Frida agent: the ORIGINAL's per-robot power accounting (docs/30).
// 0x4dc3f0 is the per-robot power step (thiscall; ecx = the robot's status object, the one the HUD
// reads +0xa4 air / +0xa8 battery / +0xbc dot from).  Layout recovered from its code:
//   +0x18 component list (begin [+0x1c], end [+0x20], 4-byte handles -> *handle = component)
//   +0x84 air total   +0x88 elec total   +0x8c air now   +0x90 elec now
//   +0x94/+0x98 air demand/supply sums of this step   +0x9c/+0xa0 elec demand/supply sums
//   +0xa4 air fraction  +0xa8 battery fraction  +0xbc dot
// Per power source (Battery, AirTank): +0x280 total, +0x284 now, +0x288 drain/s, +0x28c rate fraction, +0x290 avail.
'use strict';
const robots = new Map();
Interceptor.attach(ptr('0x4dc3f0'), { onEnter() { robots.set(this.context.ecx.toString(), this.context.ecx); } });
const f = (p, o) => p.add(o).readFloat();
const t0 = Date.now();
const KNOWN = { '0x6a90b4': 'Battery', '0x6a9134': 'AirTank', '0x6ab214': 'SpinMotor' };
function comps(r) {
  const out = [];
  const b = r.add(0x1c).readPointer(), e = r.add(0x20).readPointer();
  for (let p = b; p.compare(e) < 0 && out.length < 64; p = p.add(4)) {
    try {
      const h = p.readPointer(); const c = h.readPointer(); const vt = c.readPointer();
      const name = KNOWN['0x' + vt.toString(16).replace(/^0x/, '')] || vt.toString();
      out.push({ c: c.toString(), vt: name, f: [0x280, 0x284, 0x288, 0x28c, 0x290, 0x294, 0x298].map(o => f(c, o)),
                 i: [0x298, 0x29c, 0x2a0].map(o => c.add(o).readS32()) });
    } catch (e) {}
  }
  return out;
}
setInterval(() => {
  robots.forEach((r, k) => {
    try {
      const rows = [0x84, 0x88, 0x8c, 0x90, 0x94, 0x98, 0x9c, 0xa0, 0xa4, 0xa8, 0xb0, 0xb4].map(o => f(r, o));
      send({ t: Date.now() - t0, r: k, v: rows, dot: r.add(0xbc).readS32(), flag80: r.add(0x80).readU8(), comps: comps(r) });
    } catch (e) { send({ err: e.toString() }); }
  });
}, 200);
