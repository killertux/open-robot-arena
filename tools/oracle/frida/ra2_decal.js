// Frida agent: the Paint Shop decal's band restore (docs/36 9.31 / 9.33). `FUN_00426130` (decal) first calls `FUN_00423ea0` (a snapshot of the
// skin: `this+0xd4 = new FUN_005a87c0(this+0xd0's texture at +0x2b4)`), draws the quad into the skin render target, then `CopyRects` (`FUN_005b30a0`,
// `IDirect3DDevice8` vtable +0x70) four bands outside the decal rectangle R from the snapshot. This logs, per decal: the paint screen, the snapshot
// and skin texture objects (width / height / format fields), R, the four rectangles, the CopyRects call, and the snapshot's pixels
// (LockRect level 0, when it can be locked) as binary `send` payloads.
'use strict';
const DUMP = true; // false: log only (LockRect on the snapshot changes what the live game draws, docs/36 9.33)
function rd(p, o) { return p.add(o).readU32(); }
function objInfo(o) {
  if (o.isNull()) return null;
  const raw = []; for (let i = 0x1c; i < 0x60; i += 4) raw.push(rd(o, i));
  return { p: o.toString(), vt: rd(o, 0).toString(16), tex: rd(o, 8).toString(16), raw };
}
function lockDump(o, tag) {
  if (!DUMP) return;
  try {
    const tex = ptr(rd(o, 8)); if (tex.isNull()) return;
    const vt = tex.readPointer();
    const lock = new NativeFunction(vt.add(16 * 4).readPointer(), 'uint32', ['pointer', 'uint32', 'pointer', 'pointer', 'uint32'], 'stdcall');
    const unlock = new NativeFunction(vt.add(17 * 4).readPointer(), 'uint32', ['pointer', 'uint32'], 'stdcall');
    const lr = Memory.alloc(8);
    const hr = lock(tex, 0, lr, ptr(0), 0x10);
    if (hr !== 0) { send({ ev: 'lockfail', tag, hr: hr.toString(16) }); return; }
    const pitch = lr.readS32(), bits = lr.add(4).readPointer();
    const w = 256, h = 256;
    send({ ev: 'pixels', tag, pitch, w, h }, bits.readByteArray(pitch * h));
    unlock(tex, 0);
  } catch (e) { send({ ev: 'lockerr', tag, e: String(e) }); }
}
let thisScreen = null;
Interceptor.attach(ptr('0x426130'), {
  onEnter() { thisScreen = this.context.ecx; this.th = thisScreen;
    send({ ev: 'decal-enter', t: Date.now(), th: thisScreen.toString(), size: this.context.esp.add(4).readFloat(), d0: rd(thisScreen, 0xd0).toString(16), d4: rd(thisScreen, 0xd4).toString(16), d104: rd(thisScreen, 0x104).toString(16) });
    const s = ptr(rd(this.th, 0xd4)); if (!s.isNull()) { send({ ev: 'snap-before', o: objInfo(s) }); lockDump(s, 'snap-before'); } },
  onLeave() { send({ ev: 'decal-leave' }); } });
Interceptor.attach(ptr('0x423ea0'), {
  onEnter() { this.th = this.context.ecx; send({ ev: 'snapshot-enter', th: this.th.toString(), ret: this.returnAddress.toString(16) }); },
  onLeave() { const s = ptr(rd(this.th, 0xd4)); send({ ev: 'snapshot-leave', o: s.isNull() ? null : objInfo(s) });
    if (!s.isNull()) lockDump(s, 'snap-new'); } });
// after `CALL 0x4287e0`: EAX points at R (left, top, right, bottom)
Interceptor.attach(ptr('0x42641b'), { onEnter() { const p = this.context.eax; send({ ev: 'R', r: [0, 4, 8, 12].map(o => p.add(o).readS32()) }); } });
// before the CopyRects call: EDX = the four rectangles
Interceptor.attach(ptr('0x4264b9'), { onEnter() { const p = this.context.edx; const r = []; for (let i = 0; i < 16; i++) r.push(p.add(i * 4).readS32());
  send({ ev: 'rects', r, d4: objInfo(ptr(rd(thisScreen, 0xd4))), skin: objInfo(ptr(rd(ptr(rd(thisScreen, 0xd0)), 0x2b4))) }); } });
Interceptor.attach(ptr('0x5b30a0'), { onEnter() { const sp = this.context.esp; const a = []; for (let i = 0; i < 6; i++) a.push(sp.add(4 + i * 4).readU32().toString(16));
  send({ ev: 'copyrects', ecx: this.context.ecx.toString(), a, d4: rd(thisScreen, 0xd4).toString(16), d0: rd(thisScreen, 0xd0).toString(16) }); } });
