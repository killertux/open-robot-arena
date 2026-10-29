// Frida agent: the ORIGINAL's floating damage numbers (docs/37 section 7).
// 0x4178a0 popup request (player, amount, store), 0x417940 accepted request, 0x413bd0 flush of an
// accumulated entry into a Text3D object (0x198 bytes, vtable 0x6a5598) with a mover (0x5a3780) and a
// colour fader (0x5a3c00), 0x4039b0 registers the effect, 0x5a38d0 mover step.
'use strict';
const t0 = Date.now();
const f = (p, o) => p.add(o).readFloat();
const v3 = (p, o) => [f(p, o), f(p, o + 4), f(p, o + 8)];
const v4 = (p, o) => [f(p, o), f(p, o + 4), f(p, o + 8), f(p, o + 12)];
const log = o => { o.t = Date.now() - t0; send(o); };
let live = [];
let armed = 0;

Interceptor.attach(ptr('0x4178a0'), { onEnter(a) {
  const s = this.context.esp;
  log({ ev: 'request', id: s.add(4).readS32(), amount: f(s, 8), p5: s.add(0x14).readU8(), p6: s.add(0x18).readU8(),
        v1: v3(s.add(0xc).readPointer(), 0), v2: v3(s.add(0x10).readPointer(), 0) });
} });
Interceptor.attach(ptr('0x417940'), { onEnter(a) {
  const s = this.context.esp;
  log({ ev: 'accepted', id: s.add(4).readS32(), amount: f(s, 8), flag: s.add(0x14).readU8() });
} });
Interceptor.attach(ptr('0x413bd0'), { onEnter(a) {
  const it = this.context.esp.add(4).readPointer();
  let e = null;
  try { e = it.readPointer(); } catch (x) {}
  const row = { ev: 'flush', it: it.toString(), e: e && e.toString() };
  try {
    row.id = e.readS32(); row.p4 = v4(e, 4); row.p14 = v4(e, 0x14);
    row.amount = f(e, 0x24); row.flag = e.add(0x28).readU8(); row.born = f(e, 0x2c);
  } catch (x) { row.err = x.toString(); }
  armed = Date.now() + 1500;
  log(row);
} });
Interceptor.attach(ptr('0x414170'), { onEnter(a) { log({ ev: 'size', v: this.context.esp.add(4).readS32() }); } });
Interceptor.attach(ptr('0x414190'), { onEnter(a) { log({ ev: 'scale', v: f(this.context.esp, 4) }); } });
Interceptor.attach(ptr('0x4141a0'), { onEnter(a) { log({ ev: 'duration', v: f(this.context.esp, 4) }); } });
Interceptor.attach(ptr('0x5a4c00'), { onEnter(a) { log({ ev: 'colour', v: v4(this.context.esp.add(4).readPointer(), 4) }); } });
Interceptor.attach(ptr('0x5a3780'), { onEnter(a) {
  const s = this.context.esp;
  log({ ev: 'mover', a: v3(s.add(4).readPointer(), 0), b: v3(s.add(8).readPointer(), 0), s: f(s, 0xc), e: f(s, 0x10) });
} });
Interceptor.attach(ptr('0x5a3c00'), { onEnter(a) {
  const s = this.context.esp;
  log({ ev: 'fader', a: v4(s.add(4).readPointer(), 4), b: v4(s.add(8).readPointer(), 4), s: f(s, 0xc), e: f(s, 0x10) });
} });
Interceptor.attach(ptr('0x4039b0'), { onEnter(a) {
  const o = this.context.esp.add(4).readPointer();
  if (Date.now() > armed) return;
  const txt = o.add(0x104).readPointer();
  live.push(txt);
  log({ ev: 'register', obj: o.toString(), txt: txt.toString(), wrap: Array.from({ length: 0x42 }, (_, i) => f(o, i * 4)),
        dump: Array.from({ length: 0x66 }, (_, i) => f(txt, i * 4)) });
} });
// mover step: obj, t
Interceptor.attach(ptr('0x5a38d0'), { onEnter(a) {
  const s = this.context.esp;
  const o = s.add(4).readPointer();
  if (!live.some(x => x.equals(o))) return;
  log({ ev: 'step', obj: o.toString(), tt: f(s, 8), ecx: this.context.ecx.toString() });
} });
// the text object's own virtual +0x14 (set position)
const setpos = ptr('0x6a5598').add(0x14).readPointer();
Interceptor.attach(setpos, { onEnter(a) {
  if (Date.now() > armed && !live.some(x => x.equals(this.context.ecx))) return;
  log({ ev: 'setpos', fn: setpos.toString(), obj: this.context.ecx.toString(), v: v3(this.context.esp.add(4).readPointer(), 0) });
} });
// poll registered objects' raw floats
setInterval(() => {
  for (const o of live.slice(-6)) {
    try { log({ ev: 'poll', obj: o.toString(), d: Array.from({ length: 0x66 }, (_, i) => f(o, i * 4)) }); } catch (x) {}
  }
}, 100);
