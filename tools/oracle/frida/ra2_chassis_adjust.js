// Frida agent: the Bot Lab's chassis side-adjust (screen 4, FUN_0044d1b0 = a top-outline point moved).
// Logs each call (index, new point, flag), the two face normals the bend test dots (FUN_0044d680, limit
// 0.75 at 0x6a69d8), the verdict, the alert text (FUN_0043e7c0) and, on the first call, the lab's
// vectors: the top points (this+0x280), the 12-byte side records (this+0x2c8: u16 vertex ... int side).
'use strict';
const t0 = Date.now();
let first = true;
const depth = [];
const rd = (p, n) => { const o = []; for (let i = 0; i < n; i++) o.push(p.add(i * 4).readFloat()); return o; };
Interceptor.attach(ptr('0x44d1b0'), {
  onEnter(args) {
    const lab = this.context.ecx;
    const pt = args[1];
    const e = { ev: 'adjust', t: Date.now() - t0, idx: args[0].toInt32(), pt: [pt.readS32(), pt.add(4).readS32()],
                flag: args[2].toInt32() & 0xff, lab: lab.toString() };
    e.tris = lab.add(0x2a8).readU32(); e.mesh = lab.add(0x2ac).readU32();
    e.height = lab.add(0x2ec).readFloat(); e.area = lab.add(0x300).readFloat();
    if (first) {
      first = false;
      try {
      const rb = lab.add(0x2c8 + 4).readPointer(), re = lab.add(0x2c8 + 8).readPointer();
      e.rec = [];
      for (let p = rb; p.compare(re) < 0; p = p.add(12)) e.rec.push([p.readU16(), p.add(2).readU16(), p.add(4).readU32(), p.add(8).readS32()]);
      } catch (x) { e.recerr = String(x); }
    }
    this.e = e; depth.push(e);
    send(e);
  },
  onLeave(ret) { const e = depth.pop(); send({ ev: 'verdict', t: Date.now() - t0, idx: e.idx, flag: e.flag, ok: (ret.toInt32() & 0xff) }); },
});
Interceptor.attach(ptr('0x44d680'), { onEnter(args) { this.a = args[0]; this.b = args[1]; },
  onLeave(ret) { send({ ev: 'dot', n0: rd(this.a, 3), n1: rd(this.b, 3) }); } });
Interceptor.attach(ptr('0x43e7c0'), { onEnter(args) {
  let s = null; try { const p = args[0]; const d = p.add(4).readPointer(); s = d.readCString(); } catch (e) { s = 'err ' + e; }
  send({ ev: 'alert', t: Date.now() - t0, s }); } });
